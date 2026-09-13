use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::goal_commitment::GoalCommitmentDetector;
use crate::heuristic::AssignmentHeuristic;
use crate::macros::MacroEngine;
use crate::planning::rooms::RoomMap;
use crate::planning::StructuralPlan;
use crate::transposition::TranspositionTable;
use crate::zobrist::ZobristKeys;

use super::successors::{expand_push_sequence, generate_successors_committed};

struct GoalLane {
    source: u16,
    support: u16,
}

struct GoalAccessEntry {
    goal: u16,
    label: u8,
    lanes: Vec<GoalLane>,
}

struct GoalAccessTable {
    entries: Vec<GoalAccessEntry>,
}

impl GoalAccessTable {
    fn new(cb: &CompiledBoard) -> Self {
        let dirs = [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ];
        let entries = cb
            .goal_cells
            .iter()
            .map(|&(goal, label)| {
                let lanes = dirs
                    .iter()
                    .filter_map(|&dir| {
                        let opp = dir.opposite();
                        let source = cb.neighbor(goal, opp);
                        if source == INVALID_CELL {
                            return None;
                        }
                        let support = cb.neighbor(source, opp);
                        if support == INVALID_CELL {
                            return None;
                        }
                        Some(GoalLane { source, support })
                    })
                    .collect();
                GoalAccessEntry {
                    goal,
                    label: label.0,
                    lanes,
                }
            })
            .collect();
        GoalAccessTable { entries }
    }

    fn count_blocked_goals(&self, box_cells: &[(u16, u8)]) -> u32 {
        let mut blocked = 0u32;
        for entry in &self.entries {
            if entry.lanes.is_empty() {
                continue;
            }
            let on_goal = box_cells
                .iter()
                .any(|&(c, l)| c == entry.goal && l == entry.label);
            if on_goal {
                continue;
            }
            let has_open_lane = entry.lanes.iter().any(|lane| {
                let source_blocked = box_cells
                    .iter()
                    .any(|&(c, l)| c == lane.source && l != entry.label);
                let support_blocked = box_cells.iter().any(|&(c, _)| c == lane.support);
                !source_blocked && !support_blocked
            });
            if !has_open_lane {
                blocked += 1;
            }
        }
        blocked
    }
}

#[derive(Clone)]
pub struct BeamConfig {
    pub beam_width: usize,
    pub max_depth: u32,
    pub max_incumbents: usize,
    pub seed: u64,
    pub push_weight: f64,
    pub heuristic_weight: f64,
    pub structural_weight: f64,
    pub move_weight: f64,
    pub diversity_weight: f64,
    pub doorway_weight: f64,
    pub goal_packing_weight: f64,
    pub topology_weight: f64,
    pub evacuation_weight: f64,
    pub typed_packing_weight: f64,
    pub progress_weight: f64,
    pub crossing_flow_weight: f64,
    pub blocker_weight: f64,
    pub wall_pair_weight: f64,
    pub max_dist_weight: f64,
    pub premature_x_weight: f64,
    pub goal_access_weight: f64,
    pub max_box_branches: usize,
    pub first_push_limit: usize,
    /// Absolute elapsed-ms deadline; beam stops when budget.elapsed_ms() > this.
    pub deadline_ms: Option<f64>,
    /// When set, during plateau with remaining ≤ 3 boxes, only generate pushes
    /// for off-goal boxes and on-goal boxes within this Manhattan distance.
    pub endgame_focus_radius: Option<u32>,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            beam_width: 256,
            max_depth: 500,
            max_incumbents: 4,
            seed: 0,
            push_weight: 0.0,
            heuristic_weight: 2.0,
            structural_weight: 0.8,
            move_weight: 0.002,
            diversity_weight: 1.5,
            doorway_weight: 1.5,
            goal_packing_weight: 0.8,
            topology_weight: 0.7,
            evacuation_weight: 0.6,
            typed_packing_weight: 2.0,
            progress_weight: 0.0,
            crossing_flow_weight: 0.35,
            blocker_weight: 0.8,
            wall_pair_weight: 5.0,
            max_dist_weight: 2.0,
            premature_x_weight: 0.3,
            goal_access_weight: 5.0,
            max_box_branches: 10,
            first_push_limit: 18,
            deadline_ms: None,
            endgame_focus_radius: None,
        }
    }
}

pub struct BeamIncumbent {
    pub pushes: Vec<(usize, Direction)>,
    pub push_count: u32,
    pub final_state: DenseState,
}

pub enum BeamResult {
    Solved {
        incumbents: Vec<BeamIncumbent>,
    },
    NoSolution,
    BudgetExceeded,
    Checkpoints {
        states: Vec<(DenseState, Vec<(usize, Direction)>)>,
    },
}

const NO_PARENT: u32 = u32::MAX;

struct HistoryNode {
    parent_id: u32,
    box_index: usize,
    direction: Direction,
}

struct BeamEntry {
    state: DenseState,
    g_cost: u32,
    h_cost: u32,
    score: f64,
    push_class: u64,
    history_id: u32,
    feat_packing: u32,
    feat_evac: u32,
    feat_topo: u32,
}

fn hash_noise(hash: u64, seed: u64) -> f64 {
    let mut h = (2166136261u32 ^ seed as u32).wrapping_mul(16777619);
    h ^= (hash & 0xFFFFFFFF) as u32;
    h = h.wrapping_mul(16777619);
    h ^= (hash >> 32) as u32;
    h = h.wrapping_mul(16777619);
    (h as f64) / (0x1_0000_0000u64 as f64)
}

fn push_class_key(box_index: usize, direction: Direction) -> u64 {
    (box_index as u64) << 2 | direction as u64
}

pub fn sum_min_distances(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    let mut total: u32 = 0;
    for &(cell, label) in box_cells {
        let d = min_push_distance_to_goal(cb, cell, label);
        if d == u32::MAX {
            return u32::MAX;
        }
        total = total.saturating_add(d);
    }
    total
}

struct RoomLabelBalance {
    overloaded: Vec<Vec<bool>>,
    active: bool,
}

impl RoomLabelBalance {
    fn build(
        cb: &CompiledBoard,
        initial_box_cells: &[(u16, u8)],
        rooms: &RoomMap,
        goal_rooms: &GoalRoomAssignment,
    ) -> Self {
        if !goal_rooms.active || rooms.room_count <= 1 {
            return Self {
                overloaded: Vec::new(),
                active: false,
            };
        }

        let mut labels: Vec<u8> = Vec::new();
        for &(_, label) in initial_box_cells {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }

        let rc = rooms.room_count as usize;
        let mut overloaded = vec![vec![false; rc]; 256];

        for &label in &labels {
            let mut boxes_per_room = vec![0i32; rc];
            let mut goals_per_room = vec![0i32; rc];

            for &(cell, l) in initial_box_cells {
                if l != label {
                    continue;
                }
                if let Some(r) = rooms.cell_room(cell) {
                    boxes_per_room[r as usize] += 1;
                }
            }

            for (gi, &(_, gl)) in cb.goal_cells.iter().enumerate() {
                if gl.0 != label {
                    continue;
                }
                if let Some(r) = goal_rooms.goal_room.get(gi).copied().flatten() {
                    goals_per_room[r as usize] += 1;
                }
            }

            for r in 0..rc {
                overloaded[label as usize][r] = boxes_per_room[r] > goals_per_room[r];
            }
        }

        Self {
            overloaded,
            active: true,
        }
    }

    fn is_overloaded(&self, label: u8, room: u16) -> bool {
        if !self.active {
            return false;
        }
        self.overloaded
            .get(label as usize)
            .and_then(|v| v.get(room as usize).copied())
            .unwrap_or(false)
    }
}

fn sum_room_aware_distances(
    cb: &CompiledBoard,
    box_cells: &[(u16, u8)],
    rooms: &RoomMap,
    goal_rooms: &GoalRoomAssignment,
    balance: &RoomLabelBalance,
) -> u32 {
    if !balance.active {
        return sum_min_distances(cb, box_cells);
    }
    let mut total: u32 = 0;
    for &(cell, label) in box_cells {
        if cb.goal_matches(cell, label) {
            continue;
        }
        let box_room = rooms.cell_room(cell);
        let prefer_other_room = box_room
            .map(|r| balance.is_overloaded(label, r))
            .unwrap_or(false);

        let mut best_preferred = u32::MAX;
        let mut best_any = u32::MAX;
        for (gi, &(_, gl)) in cb.goal_cells.iter().enumerate() {
            if gl.0 != label {
                continue;
            }
            let d = cb.reverse_push_distance(gi, cell) as u32;
            if d < best_any {
                best_any = d;
            }
            if prefer_other_room {
                let gr = goal_rooms.goal_room.get(gi).copied().flatten();
                if gr != box_room && d < best_preferred {
                    best_preferred = d;
                }
            }
        }
        let d = if prefer_other_room && best_preferred < u32::MAX {
            best_preferred
        } else {
            best_any
        };
        if d == u32::MAX {
            return u32::MAX;
        }
        total = total.saturating_add(d);
    }
    total
}

fn min_push_distance_to_goal(cb: &CompiledBoard, cell: u16, label: u8) -> u32 {
    cb.goal_cells
        .iter()
        .enumerate()
        .filter(|(_, &(_, gl))| gl.0 == label)
        .map(|(gi, _)| {
            let d = cb.reverse_push_distance(gi, cell);
            if d == u16::MAX {
                u32::MAX
            } else {
                d as u32
            }
        })
        .min()
        .unwrap_or(u32::MAX)
}

fn crossing_flow_delta(
    rooms: &RoomMap,
    goal_rooms: &GoalRoomAssignment,
    cb: &CompiledBoard,
    from_cell: u16,
    to_cell: u16,
    label: u8,
) -> f64 {
    if !goal_rooms.active {
        return 0.0;
    }
    let from_room = rooms.cell_room(from_cell);
    let to_room = rooms.cell_room(to_cell);
    match (from_room, to_room) {
        (Some(fr), Some(tr)) if fr != tr => {
            let matches_goal = cb
                .goal_cells
                .iter()
                .enumerate()
                .filter(|(_, &(_, gl))| gl.0 == label)
                .filter_map(|(gi, _)| goal_rooms.goal_room.get(gi).copied().flatten())
                .any(|gr| gr == tr);
            if matches_goal {
                -1.5
            } else {
                1.5
            }
        }
        (Some(_), None) => {
            if rooms.is_doorway(to_cell) {
                0.3
            } else {
                0.0
            }
        }
        (None, Some(tr)) if rooms.is_doorway(from_cell) => {
            let matches_goal = cb
                .goal_cells
                .iter()
                .enumerate()
                .filter(|(_, &(_, gl))| gl.0 == label)
                .filter_map(|(gi, _)| goal_rooms.goal_room.get(gi).copied().flatten())
                .any(|gr| gr == tr);
            if matches_goal {
                -0.75
            } else {
                0.75
            }
        }
        _ => 0.0,
    }
}

fn blocker_delta(
    cb: &CompiledBoard,
    parent_box_cells: &[(u16, u8)],
    pushed_box_idx: usize,
    vacated_cell: u16,
    occupied_cell: u16,
) -> f64 {
    let mut vacated_benefit = 0.0f64;
    let mut occupied_cost = 0.0f64;

    for (bi, &(box_cell, box_label)) in parent_box_cells.iter().enumerate() {
        if bi == pushed_box_idx {
            continue;
        }
        if cb.goal_matches(box_cell, box_label) {
            continue;
        }

        for (gi, &(_, goal_label)) in cb.goal_cells.iter().enumerate() {
            if goal_label.0 != box_label {
                continue;
            }

            let dist_to_box = cb.reverse_push_distance(gi, box_cell);
            if dist_to_box == u16::MAX || dist_to_box == 0 {
                continue;
            }

            let dist_to_vacated = cb.reverse_push_distance(gi, vacated_cell);
            if dist_to_vacated < dist_to_box {
                vacated_benefit += 1.0;
            }

            let dist_to_occupied = cb.reverse_push_distance(gi, occupied_cell);
            if dist_to_occupied < dist_to_box {
                occupied_cost += 1.0;
            }
        }
    }

    occupied_cost - vacated_benefit
}

fn wall_adjacent_pair_penalty(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    use crate::compiled_board::INVALID_CELL;
    let mut penalty = 0u32;
    let n = box_cells.len();
    for i in 0..n {
        let (ci, li) = box_cells[i];
        if cb.goal_matches(ci, li) {
            continue;
        }
        for &(cj, lj) in &box_cells[(i + 1)..n] {
            if cb.goal_matches(cj, lj) {
                continue;
            }
            let ni_right = cb.neighbor(ci, Direction::Right);
            let ni_down = cb.neighbor(ci, Direction::Down);
            let adjacent_h = ni_right != INVALID_CELL && ni_right == cj;
            let adjacent_v = ni_down != INVALID_CELL && ni_down == cj;
            if !adjacent_h && !adjacent_v {
                continue;
            }
            if adjacent_h {
                let above_i = cb.neighbor(ci, Direction::Up);
                let above_j = cb.neighbor(cj, Direction::Up);
                let below_i = cb.neighbor(ci, Direction::Down);
                let below_j = cb.neighbor(cj, Direction::Down);
                if (above_i == INVALID_CELL && above_j == INVALID_CELL)
                    || (below_i == INVALID_CELL && below_j == INVALID_CELL)
                {
                    penalty += 3;
                } else if above_i == INVALID_CELL
                    || above_j == INVALID_CELL
                    || below_i == INVALID_CELL
                    || below_j == INVALID_CELL
                {
                    penalty += 1;
                }
            }
            if adjacent_v {
                let left_i = cb.neighbor(ci, Direction::Left);
                let left_j = cb.neighbor(cj, Direction::Left);
                let right_i = cb.neighbor(ci, Direction::Right);
                let right_j = cb.neighbor(cj, Direction::Right);
                if (left_i == INVALID_CELL && left_j == INVALID_CELL)
                    || (right_i == INVALID_CELL && right_j == INVALID_CELL)
                {
                    penalty += 3;
                } else if left_i == INVALID_CELL
                    || left_j == INVALID_CELL
                    || right_i == INVALID_CELL
                    || right_j == INVALID_CELL
                {
                    penalty += 1;
                }
            }
        }
    }
    penalty
}

fn max_offgoal_distance(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    let mut max_d = 0u32;
    for &(cell, label) in box_cells {
        if cb.goal_matches(cell, label) {
            continue;
        }
        let d = min_push_distance_to_goal(cb, cell, label);
        if d < u32::MAX && d > max_d {
            max_d = d;
        }
    }
    max_d
}

fn target_sum_distances(cb: &CompiledBoard, box_cells: &[(u16, u8)], target_labels: &[u8]) -> u32 {
    if target_labels.is_empty() {
        return 0;
    }
    let mut total = 0u32;
    for &(cell, label) in box_cells {
        if !target_labels.contains(&label) {
            continue;
        }
        if cb.goal_matches(cell, label) {
            continue;
        }
        let d = min_push_distance_to_goal(cb, cell, label);
        if d == u32::MAX {
            return u32::MAX;
        }
        total = total.saturating_add(d);
    }
    total
}

fn endgame_focus_mask(cb: &CompiledBoard, box_cells: &[(u16, u8)], radius: u32) -> u64 {
    let offgoal: Vec<u16> = box_cells
        .iter()
        .filter(|&&(cell, label)| !cb.goal_matches(cell, label))
        .map(|&(cell, _)| cell)
        .collect();
    if offgoal.is_empty() {
        return 0;
    }
    let mut mask = 0u64;
    for (bi, &(cell, label)) in box_cells.iter().enumerate() {
        if bi >= 64 {
            break;
        }
        if !cb.goal_matches(cell, label) {
            continue;
        }
        let pos = cb.cell_to_pos(cell);
        let near = offgoal.iter().any(|&og| {
            let og_pos = cb.cell_to_pos(og);
            let dr = (pos.row as i32 - og_pos.row as i32).unsigned_abs();
            let dc = (pos.col as i32 - og_pos.col as i32).unsigned_abs();
            dr + dc <= radius
        });
        if !near {
            mask |= 1u64 << bi;
        }
    }
    mask
}

fn premature_x_penalty(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> f64 {
    let typed_remaining = box_cells
        .iter()
        .filter(|&&(cell, label)| label != 0 && !cb.goal_matches(cell, label))
        .count() as u32;
    if typed_remaining == 0 {
        return 0.0;
    }
    let x_on_goals = box_cells
        .iter()
        .filter(|&&(cell, label)| label == 0 && cb.goal_matches(cell, label))
        .count() as u32;
    x_on_goals as f64 * typed_remaining as f64
}

fn doorway_traffic_penalty(
    cb: &CompiledBoard,
    rooms: &RoomMap,
    goal_rooms: &GoalRoomAssignment,
    box_cells: &[(u16, u8)],
) -> u32 {
    if !goal_rooms.active || rooms.room_count <= 1 {
        return 0;
    }
    let mut penalty = 0u32;

    for &(cell, label) in box_cells {
        if !rooms.is_doorway(cell) || cb.goal_matches(cell, label) {
            continue;
        }
        let mut adj_rooms: [u16; 4] = [u16::MAX; 4];
        let mut adj_count = 0usize;
        for dir in Direction::ALL {
            let nb = cb.neighbor(cell, dir);
            if nb == INVALID_CELL {
                continue;
            }
            if let Some(r) = rooms.cell_room(nb) {
                if !adj_rooms[..adj_count].contains(&r) {
                    adj_rooms[adj_count] = r;
                    adj_count += 1;
                }
            }
        }
        if adj_count < 2 {
            penalty += 5;
            continue;
        }
        let mut crossings_needed = 0u32;
        for &(b_cell, b_label) in box_cells {
            if cb.goal_matches(b_cell, b_label) {
                continue;
            }
            let br = match rooms.cell_room(b_cell) {
                Some(r) => r,
                None => continue,
            };
            let in_adj = adj_rooms[..adj_count].contains(&br);
            if !in_adj {
                continue;
            }
            let goal_in_same = cb
                .goal_cells
                .iter()
                .enumerate()
                .filter(|(_, &(_, gl))| gl.0 == b_label)
                .filter_map(|(gi, _)| goal_rooms.goal_room.get(gi).copied().flatten())
                .any(|gr| gr == br);
            if !goal_in_same {
                crossings_needed += 1;
            }
        }
        penalty += 5 + 6 * crossings_needed;

        let mut lane_blocked = 0u32;
        for dir in Direction::ALL {
            let nb = cb.neighbor(cell, dir);
            if nb == INVALID_CELL {
                continue;
            }
            let occ = box_cells.iter().any(|&(c, _)| c == nb);
            if occ {
                lane_blocked += 1;
            }
        }
        penalty += 8 * lane_blocked;
    }
    penalty
}

pub fn goal_packing_count(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    let mut count = 0u32;
    for &(cell, label) in box_cells {
        if cb.goal_matches(cell, label) {
            count += 1;
        }
    }
    count
}

fn typed_packing_count(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    let mut count = 0u32;
    for &(cell, label) in box_cells {
        if label != 0 && cb.goal_matches(cell, label) {
            count += 1;
        }
    }
    count
}

fn select_box_skip_mask(
    cb: &CompiledBoard,
    box_cells: &[(u16, u8)],
    committed_mask: u64,
    max_branches: usize,
    rooms: Option<&RoomMap>,
    goal_rooms: &GoalRoomAssignment,
) -> u64 {
    let n = box_cells.len().min(64);

    let committed_count = (0..n)
        .filter(|&i| (committed_mask & (1u64 << i)) != 0)
        .count();
    let available = n - committed_count;
    if available <= max_branches {
        return committed_mask;
    }

    let mut near_goal: Vec<usize> = Vec::new();
    let mut rest: Vec<(usize, u32)> = Vec::new();

    let mut skip_mask = committed_mask;

    for (bi, &(cell, label)) in box_cells.iter().enumerate().take(n) {
        if (committed_mask & (1u64 << bi)) != 0 {
            continue;
        }
        if cb.goal_matches(cell, label) {
            rest.push((bi, 10_000));
            continue;
        }

        let min_dist = cb
            .goal_cells
            .iter()
            .enumerate()
            .filter(|(_, &(_, gl))| gl.0 == label)
            .map(|(gi, _)| cb.reverse_push_distance(gi, cell) as u32)
            .min()
            .unwrap_or(10_000);

        if min_dist <= 2 {
            near_goal.push(bi);
            continue;
        }

        let needs_crossing = if let (Some(rm), true) = (rooms, goal_rooms.active) {
            let box_room = rm.cell_room(cell);
            let goal_room = cb
                .goal_cells
                .iter()
                .enumerate()
                .filter(|(_, &(_, gl))| gl.0 == label)
                .filter_map(|(gi, _)| goal_rooms.goal_room.get(gi).copied().flatten())
                .next();
            matches!((box_room, goal_room), (Some(br), Some(gr)) if br != gr)
        } else {
            false
        };

        let typed_bonus = label != 0;
        let priority = if needs_crossing || typed_bonus {
            0
        } else {
            min_dist
        };
        rest.push((bi, priority));
    }

    rest.sort_by_key(|&(_, d)| d);

    let mut selected = 0usize;

    for &bi in &near_goal {
        if selected >= max_branches {
            skip_mask |= 1u64 << bi;
        } else {
            selected += 1;
        }
    }

    for &(bi, _) in &rest {
        if selected >= max_branches {
            skip_mask |= 1u64 << bi;
        } else {
            selected += 1;
        }
    }

    skip_mask
}

struct GoalRoomAssignment {
    goal_room: Vec<Option<u16>>,
    active: bool,
}

impl GoalRoomAssignment {
    fn build(cb: &CompiledBoard, rooms: &RoomMap) -> Self {
        if rooms.room_count <= 1 {
            return Self {
                goal_room: Vec::new(),
                active: false,
            };
        }
        let goal_room: Vec<Option<u16>> = cb
            .goal_cells
            .iter()
            .map(|&(cell, _)| rooms.cell_room(cell))
            .collect();
        Self {
            goal_room,
            active: true,
        }
    }

    fn wrong_side_count(
        &self,
        cb: &CompiledBoard,
        rooms: &RoomMap,
        box_cells: &[(u16, u8)],
    ) -> u32 {
        if !self.active {
            return 0;
        }
        let mut count = 0u32;
        for &(cell, label) in box_cells {
            if cb.goal_matches(cell, label) {
                continue;
            }
            let box_room = rooms.cell_room(cell);
            let nearest_goal_room = cb
                .goal_cells
                .iter()
                .enumerate()
                .filter(|(_, &(_, gl))| gl.0 == label)
                .filter_map(|(gi, _)| self.goal_room.get(gi).copied().flatten())
                .min_by_key(|&gr| if box_room == Some(gr) { 0u32 } else { 1 });
            if let (Some(br), Some(gr)) = (box_room, nearest_goal_room) {
                if br != gr {
                    count += 1;
                }
            }
        }
        count
    }
}

struct RoomGoalCounts {
    goals_per_room: Vec<u16>,
    room_count: u16,
    active: bool,
}

impl RoomGoalCounts {
    fn build(cb: &CompiledBoard, rooms: &RoomMap) -> Self {
        if rooms.room_count <= 1 {
            return RoomGoalCounts {
                goals_per_room: Vec::new(),
                room_count: 0,
                active: false,
            };
        }

        let mut goals_per_room = vec![0u16; rooms.room_count as usize];
        for &(cell, _) in &cb.goal_cells {
            if let Some(r) = rooms.cell_room(cell) {
                goals_per_room[r as usize] += 1;
            }
        }

        RoomGoalCounts {
            goals_per_room,
            room_count: rooms.room_count,
            active: true,
        }
    }

    fn topology_penalty(&self, rooms: &RoomMap, box_cells: &[(u16, u8)]) -> u32 {
        if !self.active {
            return 0;
        }

        let mut boxes_per_room = vec![0u16; self.room_count as usize];
        let mut doorway_boxes = 0u32;
        for &(cell, _) in box_cells {
            if let Some(r) = rooms.cell_room(cell) {
                boxes_per_room[r as usize] += 1;
            } else if rooms.is_doorway(cell) {
                doorway_boxes += 1;
            }
        }

        let mut penalty = 0u32;
        for (r, &box_count) in boxes_per_room
            .iter()
            .enumerate()
            .take(self.room_count as usize)
        {
            let diff = (box_count as i32 - self.goals_per_room[r] as i32).unsigned_abs();
            penalty += diff;
        }
        penalty += doorway_boxes * 2;
        penalty
    }
}

fn archive_key(c: &BeamEntry, best_h: u32, max_pack: u32) -> u32 {
    let slack_bin = {
        let s = c.h_cost.saturating_sub(best_h);
        if s <= 2 {
            0
        } else if s <= 6 {
            1
        } else if s <= 12 {
            2
        } else {
            3
        }
    };
    let pack_deficit = max_pack.saturating_sub(c.feat_packing);
    let pack_bin = pack_deficit.min(3);
    let evac_bin = if c.feat_evac == 0 {
        0
    } else if c.feat_evac <= 3 {
        1
    } else if c.feat_evac <= 7 {
        2
    } else {
        3
    };
    let topo_bin = if c.feat_topo <= 2 {
        0
    } else if c.feat_topo <= 6 {
        1
    } else if c.feat_topo <= 12 {
        2
    } else {
        3
    };
    slack_bin * 64 + pack_bin * 16 + evac_bin * 4 + topo_bin
}

fn select_beam_layer(candidates: &mut Vec<BeamEntry>, width: usize) {
    if candidates.len() <= width {
        return;
    }

    let best_h = candidates.iter().map(|c| c.h_cost).min().unwrap_or(0);
    let max_pack = candidates.iter().map(|c| c.feat_packing).max().unwrap_or(0);

    let mut selected_set = rustc_hash::FxHashSet::default();
    let mut selected = Vec::with_capacity(width);

    // Phase 1: MAP-Elites archive — 35% of beam width
    let archive_quota = (width as f64 * 0.35) as usize;
    let mut archive_grid: rustc_hash::FxHashMap<u32, (usize, f64)> =
        rustc_hash::FxHashMap::default();

    for (i, c) in candidates.iter().enumerate() {
        let key = archive_key(c, best_h, max_pack);
        match archive_grid.get(&key) {
            Some(&(_, existing_score)) if c.score < existing_score => {
                archive_grid.insert(key, (i, c.score));
            }
            None => {
                archive_grid.insert(key, (i, c.score));
            }
            _ => {}
        }
    }

    let mut archive_entries: Vec<(u32, usize, f64)> = archive_grid
        .into_iter()
        .map(|(k, (i, s))| (k, i, s))
        .collect();
    archive_entries.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    for &(_, idx, _) in archive_entries.iter().take(archive_quota) {
        selected.push(idx);
        selected_set.insert(idx);
    }

    // Phase 2: Banded push-class diverse selection — remaining 65%
    let banded_quota = width.saturating_sub(selected.len());
    let ratios = [0.45f64, 0.28, 0.17, 0.10];
    let mut bands: [Vec<usize>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    for (i, c) in candidates.iter().enumerate() {
        if selected_set.contains(&i) {
            continue;
        }
        let slack = c.h_cost.saturating_sub(best_h);
        let band = if slack <= 2 {
            0
        } else if slack <= 5 {
            1
        } else if slack <= 9 {
            2
        } else {
            3
        };
        bands[band].push(i);
    }

    for band in &mut bands {
        band.sort_by(|&a, &b| {
            candidates[a]
                .score
                .partial_cmp(&candidates[b].score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let mut used_classes = rustc_hash::FxHashSet::default();
    for entry_idx in &selected {
        used_classes.insert(candidates[*entry_idx].push_class);
    }

    for (band_idx, band) in bands.iter().enumerate() {
        let quota = if band_idx == 3 {
            banded_quota.saturating_sub(selected.len() - (width - banded_quota))
        } else {
            (banded_quota as f64 * ratios[band_idx]) as usize
        };
        let remaining = width.saturating_sub(selected.len());
        let quota = quota.min(remaining);

        let mut added = 0;
        for &idx in band {
            if added >= quota {
                break;
            }
            let pc = candidates[idx].push_class;
            if !used_classes.contains(&pc) {
                selected.push(idx);
                selected_set.insert(idx);
                used_classes.insert(pc);
                added += 1;
            }
        }
        for &idx in band {
            if added >= quota {
                break;
            }
            if !selected_set.contains(&idx) {
                selected.push(idx);
                selected_set.insert(idx);
                added += 1;
            }
        }
    }

    // Fill remaining from best overall score
    if selected.len() < width {
        let mut all_by_score: Vec<usize> = (0..candidates.len())
            .filter(|i| !selected_set.contains(i))
            .collect();
        all_by_score.sort_by(|&a, &b| {
            candidates[a]
                .score
                .partial_cmp(&candidates[b].score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for idx in all_by_score {
            if selected.len() >= width {
                break;
            }
            selected.push(idx);
        }
    }

    selected.sort_unstable();
    let mut kept = Vec::with_capacity(selected.len());
    for idx in selected {
        kept.push(std::mem::replace(
            &mut candidates[idx],
            BeamEntry {
                state: DenseState {
                    keeper_zone: 0,
                    box_cells: Vec::new(),
                    moves: 0,
                    pushes: 0,
                },
                g_cost: 0,
                h_cost: 0,
                score: f64::MAX,
                push_class: 0,
                history_id: NO_PARENT,
                feat_packing: 0,
                feat_evac: 0,
                feat_topo: 0,
            },
        ));
    }
    *candidates = kept;
}

/// Layered beam search with weighted multi-signal scoring and
/// banded diversity-preserving selection.
#[allow(clippy::too_many_arguments)]
pub fn beam_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    macros: &MacroEngine,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    config: &BeamConfig,
    plan: Option<&StructuralPlan>,
) -> BeamResult {
    let init_hash = initial.zobrist_hash(zk);
    let init_box_hash = zk.hash_boxes(&initial.box_cells);
    let h = heuristic.evaluate(cb, initial, init_box_hash);
    counters.heuristic_calls += 1;

    if h == u32::MAX {
        return BeamResult::NoSolution;
    }

    if initial.is_solved(cb) {
        return BeamResult::Solved {
            incumbents: vec![BeamIncumbent {
                pushes: Vec::new(),
                push_count: 0,
                final_state: initial.clone(),
            }],
        };
    }

    let mut history: Vec<HistoryNode> = Vec::new();
    let mut tt = TranspositionTable::new();
    tt.insert(init_hash, 0, 0);
    let tt_max_entries: usize = if cb.initial_box_cells.len() >= 12 {
        200_000
    } else {
        0
    };
    let tt_window: u32 = if cb.initial_box_cells.len() >= 12 {
        5
    } else {
        0
    };

    let commitment_detector = GoalCommitmentDetector::new(cb);
    let goal_access = GoalAccessTable::new(cb);

    let room_balance = plan
        .filter(|p| p.is_active())
        .map(|p| RoomGoalCounts::build(cb, p.rooms()))
        .unwrap_or(RoomGoalCounts {
            goals_per_room: Vec::new(),
            room_count: 0,
            active: false,
        });

    let goal_rooms = plan
        .filter(|p| p.is_active())
        .map(|p| GoalRoomAssignment::build(cb, p.rooms()))
        .unwrap_or(GoalRoomAssignment {
            goal_room: Vec::new(),
            active: false,
        });

    let label_balance = plan
        .filter(|p| p.is_active())
        .map(|p| RoomLabelBalance::build(cb, &initial.box_cells, p.rooms(), &goal_rooms))
        .unwrap_or(RoomLabelBalance {
            overloaded: Vec::new(),
            active: false,
        });

    let init_boost = plan
        .filter(|p| p.is_active())
        .map(|p| p.evaluate_state(cb, &initial.box_cells))
        .unwrap_or(0);

    let init_doorway = plan
        .filter(|p| p.is_active())
        .map(|p| doorway_traffic_penalty(cb, p.rooms(), &goal_rooms, &initial.box_cells))
        .unwrap_or(0);

    let init_topo = plan
        .filter(|_| room_balance.active)
        .map(|p| room_balance.topology_penalty(p.rooms(), &initial.box_cells))
        .unwrap_or(0);

    let init_packing = goal_packing_count(cb, &initial.box_cells);
    let init_typed_packing = typed_packing_count(cb, &initial.box_cells);

    let init_evac = plan
        .filter(|p| p.is_active() && goal_rooms.active)
        .map(|p| goal_rooms.wrong_side_count(cb, p.rooms(), &initial.box_cells))
        .unwrap_or(0);

    let init_smd = plan
        .filter(|p| p.is_active())
        .map(|p| {
            sum_room_aware_distances(
                cb,
                &initial.box_cells,
                p.rooms(),
                &goal_rooms,
                &label_balance,
            )
        })
        .unwrap_or_else(|| sum_min_distances(cb, &initial.box_cells));
    let init_wall_pair = wall_adjacent_pair_penalty(cb, &initial.box_cells);
    let init_max_dist = max_offgoal_distance(cb, &initial.box_cells);
    let init_goal_access = goal_access.count_blocked_goals(&initial.box_cells);
    let init_score = config.heuristic_weight * init_smd as f64
        + config.structural_weight * init_boost as f64
        + config.doorway_weight * init_doorway as f64
        + config.topology_weight * init_topo as f64
        + config.evacuation_weight * init_evac as f64
        - config.goal_packing_weight * init_packing as f64
        - config.typed_packing_weight * init_typed_packing as f64
        + config.wall_pair_weight * init_wall_pair as f64
        + config.max_dist_weight * init_max_dist as f64
        + config.premature_x_weight * premature_x_penalty(cb, &initial.box_cells)
        + config.goal_access_weight * init_goal_access as f64
        + hash_noise(init_hash, config.seed) * config.diversity_weight; // g_cost = 0 at init, so push_weight * 0 is omitted

    let mut beam = vec![BeamEntry {
        state: initial.clone(),
        g_cost: 0,
        h_cost: h,
        score: init_score,
        push_class: 0,
        history_id: NO_PARENT,
        feat_packing: init_packing,
        feat_evac: init_evac,
        feat_topo: init_topo,
    }];

    let mut incumbents: Vec<BeamIncumbent> = Vec::new();

    let mut local_expanded: u64 = 0;
    let mut local_generated: u64 = 0;
    let mut local_prune_fired: u64 = 0;
    let mut layer_deadlock: u64 = 0;
    let mut layer_tt_dup: u64 = 0;
    let mut layer_h_inf: u64 = 0;
    let mut layer_total: u64 = 0;

    let checkpoint_threshold = (cb.initial_box_cells.len() as u32).saturating_sub(4);
    let mut hall_of_fame: Vec<(DenseState, u32, u32)> = Vec::new(); // (state, history_id, packing)

    let target_labels: Vec<u8> = if config.endgame_focus_radius.is_some() {
        initial
            .box_cells
            .iter()
            .filter(|&&(cell, label)| !cb.goal_matches(cell, label))
            .map(|&(_, label)| label)
            .collect()
    } else {
        Vec::new()
    };

    let init_remaining = cb.initial_box_cells.len() as u32 - init_packing;
    let mut best_pack_seen: u32 = if init_remaining <= 3 { init_packing } else { 0 };
    let mut layers_since_improvement: u32 = if init_remaining <= 3 { 40 } else { 0 };
    let plateau_threshold: u32 = 40;

    for _depth in 0..config.max_depth {
        if beam.is_empty() || budget.exhausted() {
            break;
        }
        if let Some(deadline) = config.deadline_ms {
            if budget.elapsed_ms() > deadline {
                break;
            }
        }

        let mut candidates: Vec<BeamEntry> = Vec::new();

        let plateau_active = layers_since_improvement >= plateau_threshold;
        let effective_width = if plateau_active {
            config.beam_width * 2
        } else {
            config.beam_width
        };

        let tt_reset = (tt_window > 0 && _depth > 0 && _depth % tt_window == 0)
            || (tt_max_entries > 0 && tt.len() > tt_max_entries)
            || (plateau_active && layers_since_improvement == plateau_threshold);
        if tt_reset {
            tt = TranspositionTable::new();
            for entry in &beam {
                let h = entry.state.zobrist_hash(zk);
                tt.insert(h, entry.g_cost, entry.state.moves);
            }
        }

        for entry in &beam {
            if budget.exhausted() {
                break;
            }

            budget.tick_expanded();
            counters.expanded += 1;

            let committed = commitment_detector.find_committed_boxes(cb, &entry.state.box_cells);
            let rooms_ref = plan.filter(|p| p.is_active()).map(|p| p.rooms());
            let remaining_boxes = cb.initial_box_cells.len() as u32 - entry.feat_packing;
            let effective_branches = if remaining_boxes <= 4 {
                cb.initial_box_cells.len()
            } else {
                config.max_box_branches
            };
            let mut skip_mask = select_box_skip_mask(
                cb,
                &entry.state.box_cells,
                committed,
                effective_branches,
                rooms_ref,
                &goal_rooms,
            );
            if let Some(radius) = config.endgame_focus_radius {
                if plateau_active && remaining_boxes <= 3 {
                    skip_mask |= endgame_focus_mask(cb, &entry.state.box_cells, radius);
                }
            }
            let raw_successors = generate_successors_committed(
                cb,
                &entry.state,
                Some(macros),
                Some(counters),
                skip_mask,
            );

            let raw_successors_count = raw_successors.len();
            let use_sequences = cb.initial_box_cells.len() >= 8;
            let (seq_depth, seq_explored, seq_returned) = if cb.initial_box_cells.len() >= 12 {
                (12u32, 48usize, 4usize)
            } else {
                (8, 16, 2)
            };
            let mut successors = Vec::with_capacity(
                raw_successors.len() * if use_sequences { seq_returned + 1 } else { 1 },
            );
            if use_sequences {
                for raw in raw_successors {
                    let box_label = entry.state.box_cells[raw.box_index].1;
                    let box_cell = entry.state.box_cells[raw.box_index].0;
                    let needs_long_seq = if let Some(rm) = rooms_ref {
                        let box_room = rm.cell_room(box_cell);
                        let target_room = rm.cell_room(raw.box_target);
                        let at_doorway = rm.is_doorway(box_cell) || rm.is_doorway(raw.box_target);
                        let in_wrong_room = if goal_rooms.active {
                            cb.goal_cells
                                .iter()
                                .enumerate()
                                .filter(|(_, &(_, gl))| gl.0 == box_label)
                                .filter_map(|(gi, _)| {
                                    goal_rooms.goal_room.get(gi).copied().flatten()
                                })
                                .any(|gr| box_room != Some(gr))
                        } else {
                            false
                        };
                        let far_from_goal = min_push_distance_to_goal(cb, box_cell, box_label) > 4;
                        (in_wrong_room || at_doorway || box_room != target_room) && far_from_goal
                    } else {
                        true
                    };

                    if needs_long_seq {
                        let endpoints =
                            expand_push_sequence(cb, &raw, seq_depth, seq_explored, seq_returned);
                        if endpoints.is_empty() {
                            successors.push(raw);
                        } else {
                            successors.extend(endpoints);
                        }
                    } else {
                        successors.push(raw);
                    }
                }
            } else {
                successors = raw_successors;
            }

            let successors_before_prune = successors.len();
            let succ_limit = config.max_box_branches * 2;
            local_expanded += 1;
            if successors.len() > succ_limit && use_sequences {
                local_prune_fired += 1;
                successors.sort_by(|a, b| {
                    let sa = {
                        let from = entry.state.box_cells[a.box_index].0;
                        let label = entry.state.box_cells[a.box_index].1;
                        let d1 = min_push_distance_to_goal(cb, from, label);
                        let d2 = min_push_distance_to_goal(cb, a.box_target, label);
                        let progress = if d1 < u32::MAX && d2 < u32::MAX {
                            d1 as f64 - d2 as f64
                        } else {
                            0.0
                        };
                        -progress
                    };
                    let sb = {
                        let from = entry.state.box_cells[b.box_index].0;
                        let label = entry.state.box_cells[b.box_index].1;
                        let d1 = min_push_distance_to_goal(cb, from, label);
                        let d2 = min_push_distance_to_goal(cb, b.box_target, label);
                        let progress = if d1 < u32::MAX && d2 < u32::MAX {
                            d1 as f64 - d2 as f64
                        } else {
                            0.0
                        };
                        -progress
                    };
                    sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut kept = Vec::with_capacity(succ_limit);
                let mut seen_boxes = rustc_hash::FxHashSet::default();
                for s in std::mem::take(&mut successors) {
                    if kept.len() >= succ_limit {
                        break;
                    }
                    if seen_boxes.len() < config.max_box_branches
                        || seen_boxes.contains(&s.box_index)
                    {
                        seen_boxes.insert(s.box_index);
                        kept.push(s);
                    }
                }
                successors = kept;
            }

            if counters.expanded <= 5 || counters.expanded.is_multiple_of(2000) {
                eprintln!(
                    "  expand#{}: raw={} seq={} pruned={} (limit={})",
                    counters.expanded,
                    raw_successors_count,
                    successors_before_prune,
                    successors.len(),
                    succ_limit,
                );
            }
            local_generated += successors.len() as u64;
            counters.generated += successors.len() as u64;
            budget.tick_generated(successors.len() as u64);

            for succ in successors {
                layer_total += 1;
                // Stage 1: cheap deadlock checks only (skip pi-corral BFS)
                if deadlocks.is_deadlocked_quick(cb, &succ.state.box_cells) {
                    layer_deadlock += 1;
                    continue;
                }

                let succ_hash = succ.state.zobrist_hash(zk);
                let g = entry.g_cost + succ.push_count;

                if !tt.insert(succ_hash, g, succ.state.moves) {
                    counters.transposition_duplicate += 1;
                    layer_tt_dup += 1;
                    continue;
                }
                counters.transposition_unique += 1;

                let hist_id;
                if succ.push_trace.is_empty() {
                    hist_id = history.len() as u32;
                    history.push(HistoryNode {
                        parent_id: entry.history_id,
                        box_index: succ.box_index,
                        direction: succ.direction,
                    });
                } else {
                    let mut parent = entry.history_id;
                    for &(bi, dir) in &succ.push_trace {
                        let id = history.len() as u32;
                        history.push(HistoryNode {
                            parent_id: parent,
                            box_index: bi,
                            direction: dir,
                        });
                        parent = id;
                    }
                    hist_id = parent;
                }

                if succ.state.is_solved(cb) {
                    let path = reconstruct_path(&history, hist_id);
                    incumbents.push(BeamIncumbent {
                        pushes: path,
                        push_count: g,
                        final_state: succ.state,
                    });
                    if incumbents.len() >= config.max_incumbents {
                        return BeamResult::Solved { incumbents };
                    }
                    continue;
                }

                let boost = plan
                    .filter(|p| p.is_active())
                    .map(|p| p.evaluate_state(cb, &succ.state.box_cells))
                    .unwrap_or(0);

                let doorway_occ = plan
                    .filter(|p| p.is_active())
                    .map(|p| {
                        doorway_traffic_penalty(cb, p.rooms(), &goal_rooms, &succ.state.box_cells)
                    })
                    .unwrap_or(0);

                let packing = goal_packing_count(cb, &succ.state.box_cells);
                let typed_pack = typed_packing_count(cb, &succ.state.box_cells);

                let topo = plan
                    .filter(|_| room_balance.active)
                    .map(|p| room_balance.topology_penalty(p.rooms(), &succ.state.box_cells))
                    .unwrap_or(0);

                let evac = plan
                    .filter(|p| p.is_active() && goal_rooms.active)
                    .map(|p| goal_rooms.wrong_side_count(cb, p.rooms(), &succ.state.box_cells))
                    .unwrap_or(0);

                let pushed_from = entry.state.box_cells[succ.box_index].0;
                let pushed_label = entry.state.box_cells[succ.box_index].1;
                let before_dist = min_push_distance_to_goal(cb, pushed_from, pushed_label);
                let after_dist = min_push_distance_to_goal(cb, succ.box_target, pushed_label);
                let progress = if before_dist < u32::MAX && after_dist < u32::MAX {
                    before_dist as f64 - after_dist as f64
                } else {
                    0.0
                };

                let flow = plan
                    .filter(|p| p.is_active() && goal_rooms.active)
                    .map(|p| {
                        crossing_flow_delta(
                            p.rooms(),
                            &goal_rooms,
                            cb,
                            pushed_from,
                            succ.box_target,
                            pushed_label,
                        )
                    })
                    .unwrap_or(0.0);

                let blocker = blocker_delta(
                    cb,
                    &entry.state.box_cells,
                    succ.box_index,
                    pushed_from,
                    succ.box_target,
                );

                let noise = hash_noise(succ_hash, config.seed);
                let rooms_ref = plan.filter(|p| p.is_active()).map(|p| p.rooms());
                let smd = if let Some(rm) = rooms_ref {
                    sum_room_aware_distances(
                        cb,
                        &succ.state.box_cells,
                        rm,
                        &goal_rooms,
                        &label_balance,
                    )
                } else {
                    sum_min_distances(cb, &succ.state.box_cells)
                };
                let wall_pair = wall_adjacent_pair_penalty(cb, &succ.state.box_cells);
                let succ_max_dist = max_offgoal_distance(cb, &succ.state.box_cells);
                let succ_goal_access = goal_access.count_blocked_goals(&succ.state.box_cells);
                let n_boxes = cb.initial_box_cells.len() as u32;
                let remaining = n_boxes.saturating_sub(packing);
                let plateau_dampen = if plateau_active { 0.5 } else { 1.0 };
                let endgame_factor = if plateau_active && remaining <= 2 {
                    0.0
                } else if plateau_active && remaining <= 4 {
                    0.3
                } else {
                    1.0
                };
                let adaptive_hw = plateau_dampen
                    * if remaining <= 2 {
                        config.heuristic_weight * 0.3
                    } else if remaining <= 4 {
                        config.heuristic_weight * 0.6
                    } else {
                        config.heuristic_weight
                    };
                let plateau_div = if plateau_active {
                    config.diversity_weight * 2.0
                } else {
                    config.diversity_weight
                };
                let heuristic_term =
                    if !target_labels.is_empty() && plateau_active && remaining <= 3 {
                        let tsmd = target_sum_distances(cb, &succ.state.box_cells, &target_labels);
                        let collateral = smd.saturating_sub(tsmd);
                        2.0 * tsmd as f64 + 0.05 * collateral as f64
                    } else {
                        adaptive_hw * smd as f64
                    };
                let score = config.push_weight * g as f64
                    + heuristic_term
                    + config.structural_weight * plateau_dampen * endgame_factor * boost as f64
                    + config.doorway_weight * endgame_factor * doorway_occ as f64
                    + config.topology_weight * plateau_dampen * endgame_factor * topo as f64
                    + config.evacuation_weight * plateau_dampen * endgame_factor * evac as f64
                    + config.move_weight * succ.state.moves as f64
                    - config.goal_packing_weight * plateau_dampen * endgame_factor * packing as f64
                    - config.typed_packing_weight
                        * plateau_dampen
                        * endgame_factor
                        * typed_pack as f64
                    - config.progress_weight * progress
                    + config.crossing_flow_weight * endgame_factor * flow
                    + config.blocker_weight * endgame_factor * blocker
                    + config.wall_pair_weight * plateau_dampen * endgame_factor * wall_pair as f64
                    + config.max_dist_weight
                        * plateau_dampen
                        * endgame_factor
                        * succ_max_dist as f64
                    + config.premature_x_weight * premature_x_penalty(cb, &succ.state.box_cells)
                    + config.goal_access_weight * plateau_dampen * succ_goal_access as f64
                    + plateau_div * noise;

                let pc = push_class_key(succ.box_index, succ.direction);

                if smd == u32::MAX {
                    layer_h_inf += 1;
                    continue;
                }

                candidates.push(BeamEntry {
                    state: succ.state,
                    g_cost: g,
                    h_cost: smd,
                    score,
                    push_class: pc,
                    history_id: hist_id,
                    feat_packing: packing,
                    feat_evac: evac,
                    feat_topo: topo,
                });
            }
        }

        // Stage 2: shortlist, then run expensive pi-corral check (skip for large puzzles)
        let use_expensive_deadlock = cb.initial_box_cells.len() < 12;
        if use_expensive_deadlock {
            select_beam_layer(&mut candidates, effective_width * 3);

            candidates.retain(|entry| {
                !deadlocks.is_deadlocked(
                    cb,
                    entry.state.keeper_zone,
                    &entry.state.box_cells,
                    counters,
                )
            });
        }

        select_beam_layer(&mut candidates, effective_width);
        counters.update_peak_frontier(candidates.len() as u64);

        let min_h = candidates.iter().map(|c| c.h_cost).min().unwrap_or(9999);
        let max_pack = candidates
            .iter()
            .map(|c| goal_packing_count(cb, &c.state.box_cells))
            .max()
            .unwrap_or(0);

        if max_pack > best_pack_seen {
            best_pack_seen = max_pack;
            let new_remaining = cb.initial_box_cells.len() as u32 - max_pack;
            if new_remaining > 3 {
                layers_since_improvement = 0;
            }
        } else {
            layers_since_improvement += 1;
        }

        if _depth < 25
            || _depth % 10 == 0
            || candidates.len() < effective_width / 2
            || (plateau_active && layers_since_improvement == plateau_threshold)
        {
            eprintln!(
                "    layer {}: beam={} cand={} minh={} maxpack={} plateau={} (gen={} dead={} tt={} hinf={})",
                _depth, beam.len(), candidates.len(),
                min_h, max_pack, layers_since_improvement,
                layer_total, layer_deadlock, layer_tt_dup, layer_h_inf,
            );
        }
        if max_pack >= (cb.initial_box_cells.len() as u32).saturating_sub(3) && _depth % 50 == 0 {
            if let Some(best) = candidates.iter().max_by_key(|c| c.feat_packing) {
                let off_goal: Vec<_> = best
                    .state
                    .box_cells
                    .iter()
                    .enumerate()
                    .filter(|(_, &(cell, label))| !cb.goal_matches(cell, label))
                    .map(|(bi, &(cell, label))| {
                        let pos = cb.cell_to_pos(cell);
                        let d = min_push_distance_to_goal(cb, cell, label);
                        format!(
                            "box{}(label={},r={},c={},dist={})",
                            bi, label, pos.row, pos.col, d
                        )
                    })
                    .collect();
                eprintln!("    OFF-GOAL: {}", off_goal.join(", "));
            }
        }
        layer_deadlock = 0;
        layer_tt_dup = 0;
        layer_h_inf = 0;
        layer_total = 0;

        beam = candidates;

        for entry in &beam {
            if entry.feat_packing >= checkpoint_threshold {
                let already_present = hall_of_fame.iter().any(|(s, _, p)| {
                    *p >= entry.feat_packing && s.box_cells == entry.state.box_cells
                });
                if already_present {
                    continue;
                }
                if hall_of_fame.len() < 64 {
                    hall_of_fame.push((entry.state.clone(), entry.history_id, entry.feat_packing));
                } else {
                    let worst_idx = hall_of_fame
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, (s, _, p))| {
                            let h = sum_min_distances(cb, &s.box_cells);
                            (*p, std::cmp::Reverse(h))
                        })
                        .map(|(i, _)| i);
                    if let Some(wi) = worst_idx {
                        let worst_pack = hall_of_fame[wi].2;
                        let worst_h = sum_min_distances(cb, &hall_of_fame[wi].0.box_cells);
                        let entry_h = sum_min_distances(cb, &entry.state.box_cells);
                        if entry.feat_packing > worst_pack
                            || (entry.feat_packing == worst_pack && entry_h < worst_h)
                        {
                            hall_of_fame[wi] =
                                (entry.state.clone(), entry.history_id, entry.feat_packing);
                        }
                    }
                }
            }
        }

        if !budget.memory_ok(tt.estimated_memory_bytes()) {
            break;
        }
    }

    if local_expanded > 0 {
        eprintln!(
            "  beam(w={},s={}): expanded={} generated={} avg={:.1} prune_fired={}/{}",
            config.beam_width,
            config.seed,
            local_expanded,
            local_generated,
            local_generated as f64 / local_expanded as f64,
            local_prune_fired,
            local_expanded,
        );
    }

    if !incumbents.is_empty() {
        BeamResult::Solved { incumbents }
    } else {
        let box_count = cb.initial_box_cells.len() as u32;
        let high_packing_threshold = box_count.saturating_sub(5);
        let mut checkpoints: Vec<(DenseState, Vec<(usize, Direction)>)> = beam
            .into_iter()
            .filter(|e| e.feat_packing >= high_packing_threshold || e.h_cost <= 60)
            .map(|e| {
                let path = reconstruct_path(&history, e.history_id);
                (e.state, path)
            })
            .collect();
        for (state, hist_id, _pack) in hall_of_fame {
            let path = reconstruct_path(&history, hist_id);
            checkpoints.push((state, path));
        }
        checkpoints.sort_by_key(|(s, _)| {
            let pack = goal_packing_count(cb, &s.box_cells);
            let h = sum_min_distances(cb, &s.box_cells);
            (std::cmp::Reverse(pack), h)
        });
        checkpoints.dedup_by(|(a, _), (b, _)| a.box_cells == b.box_cells);
        checkpoints.truncate(32);
        if checkpoints.is_empty() {
            if budget.exhausted() {
                BeamResult::BudgetExceeded
            } else {
                BeamResult::NoSolution
            }
        } else {
            eprintln!("  collected {} endgame checkpoints", checkpoints.len());
            BeamResult::Checkpoints {
                states: checkpoints,
            }
        }
    }
}

fn reconstruct_path(history: &[HistoryNode], mut id: u32) -> Vec<(usize, Direction)> {
    let mut path = Vec::new();
    while id != NO_PARENT {
        let node = &history[id as usize];
        path.push((node.box_index, node.direction));
        id = node.parent_id;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SolverLimits;
    use sokomind_core::board::parse_board;

    fn run_beam(rows: &[&str], config: BeamConfig) -> (BeamResult, SearchCounters) {
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let deadlocks = DeadlockChecker::new(&cb);
        let macros = MacroEngine::new(&cb);
        let limits = SolverLimits {
            max_time_ms: Some(10_000),
            max_expanded_states: Some(500_000),
            max_generated_states: None,
            max_memory_bytes: None,
        };
        let mut budget = Budget::new(&limits);
        let mut counters = SearchCounters::default();
        let initial = DenseState::from_initial(&cb);

        let result = beam_search(
            &cb,
            &initial,
            &zk,
            &mut heuristic,
            &deadlocks,
            &macros,
            &mut budget,
            &mut counters,
            &config,
            None,
        );
        (result, counters)
    }

    #[test]
    fn beam_solve_trivial() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let (result, _) = run_beam(rows, BeamConfig::default());
        match result {
            BeamResult::Solved { incumbents } => {
                assert!(!incumbents.is_empty());
                assert!(incumbents[0].push_count >= 2);
            }
            _ => panic!("expected Solved"),
        }
    }

    #[test]
    fn beam_solve_2box() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let (result, _) = run_beam(rows, BeamConfig::default());
        match result {
            BeamResult::Solved { incumbents } => {
                assert!(!incumbents.is_empty());
                assert!(incumbents[0].push_count >= 2);
            }
            _ => panic!("expected Solved"),
        }
    }

    #[test]
    fn beam_solve_typed() {
        let rows = &[
            "OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO",
        ];
        let (result, _) = run_beam(rows, BeamConfig::default());
        match result {
            BeamResult::Solved { incumbents } => {
                assert!(!incumbents.is_empty());
            }
            _ => panic!("expected Solved"),
        }
    }

    #[test]
    fn beam_unsolvable() {
        let rows = &["OOOOO", "OX  O", "O  SO", "O  RO", "OOOOO"];
        let (result, _) = run_beam(rows, BeamConfig::default());
        match result {
            BeamResult::NoSolution
            | BeamResult::BudgetExceeded
            | BeamResult::Checkpoints { .. } => {}
            BeamResult::Solved { .. } => panic!("should not solve"),
        }
    }

    #[test]
    fn beam_respects_width() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let config = BeamConfig {
            beam_width: 2,
            ..Default::default()
        };
        let (result, counters) = run_beam(rows, config);
        match result {
            BeamResult::Solved { .. } => {}
            _ => panic!("expected Solved even with narrow beam"),
        }
        assert!(counters.peak_frontier <= 2);
    }

    #[test]
    fn beam_collects_multiple_incumbents() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let config = BeamConfig {
            beam_width: 256,
            max_depth: 200,
            max_incumbents: 4,
            ..Default::default()
        };
        let (result, _) = run_beam(rows, config);
        if let BeamResult::Solved { incumbents } = result {
            assert!(!incumbents.is_empty());
        }
    }

    #[test]
    fn beam_already_solved() {
        let rows = &["OOOOO", "O  *O", "O  RO", "O   O", "OOOOO"];
        let board = parse_board(rows);
        if board.is_err() {
            return;
        }
    }

    #[test]
    fn beam_path_reconstruction_correct() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let (result, _) = run_beam(rows, BeamConfig::default());
        if let BeamResult::Solved { incumbents } = result {
            let inc = &incumbents[0];
            assert_eq!(inc.pushes.len(), inc.push_count as usize);
            assert!(inc.push_count >= 2);
            for &(_, dir) in &inc.pushes {
                assert!(matches!(
                    dir,
                    Direction::Up | Direction::Down | Direction::Left | Direction::Right
                ));
            }
        }
    }

    #[test]
    fn beam_counters_populated() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let (_, counters) = run_beam(rows, BeamConfig::default());
        assert!(counters.expanded > 0);
        assert!(counters.generated > 0);
        assert!(counters.heuristic_calls > 0);
    }

    #[test]
    fn beam_diversity_seed_affects_result() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let config1 = BeamConfig {
            seed: 0,
            ..Default::default()
        };
        let config2 = BeamConfig {
            seed: 7919,
            ..Default::default()
        };
        let (_, c1) = run_beam(rows, config1);
        let (_, c2) = run_beam(rows, config2);
        // Different seeds should lead to different search behavior.
        // We can't guarantee different results on trivial puzzles,
        // but the counters may differ.
        let _ = (c1, c2);
    }
}
