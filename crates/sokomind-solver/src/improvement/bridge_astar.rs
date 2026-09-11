use sokomind_core::position::Direction;
use std::collections::BinaryHeap;
use std::cmp::Reverse;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::deadlock::DeadlockChecker;
use crate::planning::rooms::RoomMap;
use crate::reachability::{canonical_keeper, keeper_reachable};
use rustc_hash::FxHashSet;

const MAX_EXPANDED: usize = 50_000;
const MAX_SEGMENT_PUSHES: u32 = 20;

/// Re-solve segments of a push sequence between doorway crossings using A*.
///
/// Identifies points where a box crosses a doorway cell, splitting the
/// solution into segments. For each segment, runs bounded A* from the
/// pre-segment state to the post-segment box configuration. If a shorter
/// push sub-sequence is found, it replaces the original segment.
///
/// Returns the number of pushes saved.
pub fn bridge_astar_improve(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    rooms: &RoomMap,
    pushes: &mut Vec<(usize, Direction)>,
) -> u32 {
    if pushes.len() < 3 || rooms.doorway_count == 0 {
        return 0;
    }

    let initial_len = pushes.len() as u32;
    let breakpoints = find_doorway_crossings(cb, rooms, pushes);

    if breakpoints.is_empty() {
        return 0;
    }

    let mut boundaries: Vec<usize> = Vec::with_capacity(breakpoints.len() + 2);
    boundaries.push(0);
    for &bp in &breakpoints {
        if *boundaries.last().unwrap() != bp {
            boundaries.push(bp);
        }
    }
    boundaries.push(pushes.len());

    let mut total_saved = 0u32;
    let mut offset_adjustment: i32 = 0;

    for wi in 0..boundaries.len() - 1 {
        let start = (boundaries[wi] as i32 + offset_adjustment) as usize;
        let orig_end = (boundaries[wi + 1] as i32 + offset_adjustment) as usize;

        if orig_end > pushes.len() || start >= orig_end {
            continue;
        }

        let seg_len = orig_end - start;
        if seg_len as u32 > MAX_SEGMENT_PUSHES || seg_len < 2 {
            continue;
        }

        let pre = replay_to_offset(cb, pushes, start);
        let post = replay_to_offset(cb, pushes, orig_end);

        if let Some(shorter) = astar_segment(
            cb,
            deadlocks,
            &pre,
            &post.box_cells,
            seg_len as u32 - 1,
        ) {
            let saved = seg_len - shorter.len();
            pushes.splice(start..orig_end, shorter.clone());
            offset_adjustment -= saved as i32;
            total_saved += saved as u32;
        }
    }

    let final_len = pushes.len() as u32;
    initial_len.saturating_sub(final_len).max(total_saved)
}

fn find_doorway_crossings(
    cb: &CompiledBoard,
    rooms: &RoomMap,
    pushes: &[(usize, Direction)],
) -> Vec<usize> {
    let mut crossings = Vec::new();
    let mut box_cells: Vec<(u16, u8)> = cb
        .initial_box_cells
        .iter()
        .map(|&(c, l)| (c, l.0))
        .collect();
    box_cells.sort();

    for (pi, &(box_index, dir)) in pushes.iter().enumerate() {
        let box_cell = box_cells[box_index].0;
        let target = cb.neighbor(box_cell, dir);

        if rooms.is_doorway(box_cell) || (target != INVALID_CELL && rooms.is_doorway(target)) {
            crossings.push(pi);
        }

        if target != INVALID_CELL {
            let label = box_cells[box_index].1;
            box_cells[box_index] = (target, label);
            box_cells.sort();
        }
    }

    crossings
}

struct PartialState {
    keeper: u16,
    box_cells: Vec<(u16, u8)>,
}

fn replay_to_offset(
    cb: &CompiledBoard,
    pushes: &[(usize, Direction)],
    offset: usize,
) -> PartialState {
    let mut box_cells: Vec<(u16, u8)> = cb
        .initial_box_cells
        .iter()
        .map(|&(c, l)| (c, l.0))
        .collect();
    box_cells.sort();
    let mut keeper = cb.robot_cell;

    for &(box_index, dir) in &pushes[..offset] {
        let box_cell = box_cells[box_index].0;
        let target = cb.neighbor(box_cell, dir);
        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
        keeper = box_cell;
    }

    PartialState { keeper, box_cells }
}

fn astar_segment(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    pre: &PartialState,
    target_boxes: &[(u16, u8)],
    max_pushes: u32,
) -> Option<Vec<(usize, Direction)>> {
    struct ANode {
        keeper_zone: u16,
        box_cells: Vec<(u16, u8)>,
        path: Vec<(usize, Direction)>,
    }

    let keeper_zone = canonical_keeper(cb, pre.keeper, &pre.box_cells);
    let h = heuristic_to_target(cb, &pre.box_cells, target_boxes);

    let mut nodes: Vec<ANode> = Vec::new();
    let mut open: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::new();
    let mut visited: FxHashSet<(u16, Vec<(u16, u8)>)> = FxHashSet::default();

    let init_key = (keeper_zone, pre.box_cells.clone());
    visited.insert(init_key);

    nodes.push(ANode {
        keeper_zone,
        box_cells: pre.box_cells.clone(),
        path: Vec::new(),
    });
    open.push(Reverse((h, 0)));

    let mut expanded = 0usize;

    while let Some(Reverse((_f, node_id))) = open.pop() {
        let kz = nodes[node_id].keeper_zone;
        let box_cells = nodes[node_id].box_cells.clone();
        let path = nodes[node_id].path.clone();

        if box_cells == *target_boxes && path.len() < max_pushes as usize {
            return Some(path);
        }

        let g = path.len() as u32;
        if g >= max_pushes {
            continue;
        }

        expanded += 1;
        if expanded > MAX_EXPANDED {
            return None;
        }

        let reachable = keeper_reachable(cb, kz, &box_cells);

        for (bi, &(box_cell, label)) in box_cells.iter().enumerate() {
            for dir in Direction::ALL {
                let target = cb.neighbor(box_cell, dir);
                if target == INVALID_CELL {
                    continue;
                }
                if box_cells.binary_search_by_key(&target, |&(c, _)| c).is_ok() {
                    continue;
                }

                let push_from = cb.neighbor(box_cell, dir.opposite());
                if push_from == INVALID_CELL {
                    continue;
                }
                if box_cells.binary_search_by_key(&push_from, |&(c, _)| c).is_ok() {
                    continue;
                }
                if !reachable[push_from as usize] {
                    continue;
                }

                let mut new_boxes = box_cells.to_vec();
                new_boxes[bi] = (target, label);
                new_boxes.sort();

                let new_keeper = canonical_keeper(cb, box_cell, &new_boxes);

                let key = (new_keeper, new_boxes.clone());
                if !visited.insert(key) {
                    continue;
                }

                if deadlocks.is_deadlocked_quick(cb, &new_boxes) {
                    continue;
                }

                let new_g = g + 1;
                let new_h = heuristic_to_target(cb, &new_boxes, target_boxes);
                let new_f = new_g + new_h;

                let mut new_path = path.clone();
                new_path.push((bi, dir));

                let new_id = nodes.len();
                nodes.push(ANode {
                    keeper_zone: new_keeper,
                    box_cells: new_boxes,
                    path: new_path,
                });
                open.push(Reverse((new_f, new_id)));
            }
        }
    }

    None
}

fn heuristic_to_target(
    _cb: &CompiledBoard,
    current: &[(u16, u8)],
    target: &[(u16, u8)],
) -> u32 {
    current
        .iter()
        .zip(target.iter())
        .filter(|(&(c1, _), &(c2, _))| c1 != c2)
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn no_doorways_no_change() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);
        let rooms = RoomMap::analyze(&cb);

        let mut pushes = vec![(0, Direction::Up), (0, Direction::Right)];
        let saved = bridge_astar_improve(&cb, &deadlocks, &rooms, &mut pushes);
        assert_eq!(saved, 0);
    }

    #[test]
    fn empty_pushes_no_crash() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);
        let rooms = RoomMap::analyze(&cb);

        let mut pushes = Vec::new();
        let saved = bridge_astar_improve(&cb, &deadlocks, &rooms, &mut pushes);
        assert_eq!(saved, 0);
    }

    #[test]
    fn find_crossings_no_doorways() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);

        let pushes = vec![(0, Direction::Up), (0, Direction::Right)];
        let crossings = find_doorway_crossings(&cb, &rooms, &pushes);
        assert!(crossings.is_empty());
    }

    #[test]
    fn heuristic_zero_at_target() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let boxes: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        assert_eq!(heuristic_to_target(&cb, &boxes, &boxes), 0);
    }

    #[test]
    fn heuristic_positive_when_different() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let current: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        let target = vec![(cb.goal_cells[0].0, cb.goal_cells[0].1 .0)];
        let h = heuristic_to_target(&cb, &current, &target);
        assert!(h > 0);
    }
}
