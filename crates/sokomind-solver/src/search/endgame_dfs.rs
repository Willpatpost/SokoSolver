use std::time::Instant;

use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::priority_queue::MinPriorityQueue;
use crate::transposition::TranspositionTable;
use crate::zobrist::ZobristKeys;

use super::successors::generate_successors_committed;

pub enum EndgameResult {
    Solved {
        pushes: Vec<(usize, Direction)>,
    },
    NotSolved,
}

fn compute_focus_mask(cb: &CompiledBoard, box_cells: &[(u16, u8)], radius: u32) -> u64 {
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

fn quick_heuristic(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    let mut total: u32 = 0;
    for &(cell, label) in box_cells {
        if cb.goal_matches(cell, label) {
            continue;
        }
        let d = min_push_distance(cb, cell, label);
        if d == u32::MAX {
            return u32::MAX;
        }
        total = total.saturating_add(d);
    }
    total
}

fn min_push_distance(cb: &CompiledBoard, cell: u16, label: u8) -> u32 {
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

struct FocusedNode {
    state: DenseState,
    parent_hash: Option<u64>,
    push_dir: Option<Direction>,
    push_box_idx: Option<usize>,
    g_cost: u32,
    hash: u64,
}

struct FocusedClosedEntry {
    parent_hash: Option<u64>,
    push_dir: Option<Direction>,
    push_box_idx: Option<usize>,
}

/// Focused endgame A* search. Uses a lightweight sum-of-min-distances
/// heuristic (table-lookup, no Hungarian) and dynamically commits
/// on-goal boxes far from the action to reduce branching.
///
/// `heuristic_weight`: multiplier on h in f = g + w*h. Use w=1 for
/// optimal, w=2+ for greedy speed.
#[allow(clippy::too_many_arguments)]
pub fn endgame_focused_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    deadlocks: &DeadlockChecker,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    focus_radius: u32,
    heuristic_weight: u32,
) -> EndgameResult {
    let init_hash = initial.zobrist_hash(zk);
    let h = quick_heuristic(cb, &initial.box_cells);

    if h == u32::MAX {
        return EndgameResult::NotSolved;
    }

    let mut open = MinPriorityQueue::new();
    let mut tt = TranspositionTable::new();
    let mut closed: rustc_hash::FxHashMap<u64, FocusedClosedEntry> =
        rustc_hash::FxHashMap::default();

    let f0 = h.saturating_mul(heuristic_weight);
    tt.insert(init_hash, 0, 0);
    open.push(
        f0,
        FocusedNode {
            state: initial.clone(),
            parent_hash: None,
            push_dir: None,
            push_box_idx: None,
            g_cost: 0,
            hash: init_hash,
        },
    );

    let mut best_h_seen = h;

    while let Some((_, node)) = open.pop() {
        if budget.exhausted() {
            break;
        }

        if let Some(entry) = tt.get(node.hash) {
            if node.g_cost > entry.pushes {
                continue;
            }
        }

        budget.tick_expanded();
        counters.expanded += 1;

        closed.insert(
            node.hash,
            FocusedClosedEntry {
                parent_hash: node.parent_hash,
                push_dir: node.push_dir,
                push_box_idx: node.push_box_idx,
            },
        );

        if node.state.is_solved(cb) {
            let path = reconstruct_path(&closed, node.hash);
            eprintln!(
                "      endgame_focused: SOLVED in {} pushes (expanded={})",
                path.len(),
                counters.expanded
            );
            return EndgameResult::Solved { pushes: path };
        }

        let focus_mask = compute_focus_mask(cb, &node.state.box_cells, focus_radius);
        let successors =
            generate_successors_committed(cb, &node.state, None, None, focus_mask);
        counters.generated += successors.len() as u64;
        budget.tick_generated(successors.len() as u64);

        for succ in successors {
            if deadlocks.is_deadlocked_quick(cb, &succ.state.box_cells) {
                continue;
            }

            let succ_hash = succ.state.zobrist_hash(zk);
            let g = node.g_cost + 1;

            if !tt.insert(succ_hash, g, succ.state.moves) {
                counters.transposition_duplicate += 1;
                continue;
            }
            counters.transposition_unique += 1;

            if succ.state.is_solved(cb) {
                let hist_id = closed.len();
                closed.insert(
                    succ_hash,
                    FocusedClosedEntry {
                        parent_hash: Some(node.hash),
                        push_dir: Some(succ.direction),
                        push_box_idx: Some(succ.box_index),
                    },
                );
                let path = reconstruct_path(&closed, succ_hash);
                eprintln!(
                    "      endgame_focused: SOLVED in {} pushes (expanded={}, closed={})",
                    path.len(),
                    counters.expanded,
                    hist_id,
                );
                return EndgameResult::Solved { pushes: path };
            }

            let sh = quick_heuristic(cb, &succ.state.box_cells);
            if sh == u32::MAX {
                continue;
            }

            if sh < best_h_seen {
                best_h_seen = sh;
            }

            let f = g.saturating_add(sh.saturating_mul(heuristic_weight));

            open.push(
                f,
                FocusedNode {
                    state: succ.state,
                    parent_hash: Some(node.hash),
                    push_dir: Some(succ.direction),
                    push_box_idx: Some(succ.box_index),
                    g_cost: g,
                    hash: succ_hash,
                },
            );
        }

        if !budget.memory_ok(tt.estimated_memory_bytes()) {
            break;
        }
    }

    eprintln!(
        "      endgame best_h_seen={}",
        best_h_seen,
    );
    EndgameResult::NotSolved
}

/// Greedy depth-first search for endgame completion.
///
/// Sorts children by h (lowest first) with optional random perturbation,
/// commits depth-first to the best child, and backtracks only when stuck.
/// This naturally handles displacement sequences where A* wastes time
/// exploring all states at each f-level.
///
/// `h_slack`: max amount h is allowed to increase above `init_h`.
/// `depth_limit`: maximum pushes from the starting state.
/// `noise_seed`: if > 0, perturbs child ordering for randomized restarts.
#[allow(clippy::too_many_arguments)]
pub fn endgame_greedy_dfs(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    deadlocks: &DeadlockChecker,
    focus_radius: u32,
    h_slack: u32,
    depth_limit: u32,
    time_limit_ms: u64,
    noise_seed: u64,
) -> EndgameResult {
    let start = Instant::now();
    let init_h = quick_heuristic(cb, &initial.box_cells);
    if init_h == u32::MAX {
        return EndgameResult::NotSolved;
    }
    let h_ceiling = init_h.saturating_add(h_slack);

    let mut path: Vec<(usize, Direction)> = Vec::new();
    let mut visited: rustc_hash::FxHashSet<u64> = rustc_hash::FxHashSet::default();
    let init_hash = initial.zobrist_hash(zk);
    visited.insert(init_hash);

    let mut expanded = 0u64;

    let result = dfs_recurse(
        cb,
        initial,
        zk,
        deadlocks,
        focus_radius,
        h_ceiling,
        depth_limit,
        noise_seed,
        &mut path,
        &mut visited,
        &mut expanded,
        start,
        time_limit_ms,
    );

    eprintln!(
        "      greedy_dfs: expanded={} depth={} slack={} r={} seed={} {}",
        expanded,
        depth_limit,
        h_slack,
        focus_radius,
        noise_seed,
        if result { "SOLVED" } else { "not_solved" },
    );

    if result {
        EndgameResult::Solved { pushes: path }
    } else {
        EndgameResult::NotSolved
    }
}

fn fnv_mix(mut x: u64) -> u64 {
    x = x.wrapping_mul(0x100000001b3);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x
}

#[allow(clippy::too_many_arguments)]
fn dfs_recurse(
    cb: &CompiledBoard,
    state: &DenseState,
    zk: &ZobristKeys,
    deadlocks: &DeadlockChecker,
    focus_radius: u32,
    h_ceiling: u32,
    depth_remaining: u32,
    noise_seed: u64,
    path: &mut Vec<(usize, Direction)>,
    visited: &mut rustc_hash::FxHashSet<u64>,
    expanded: &mut u64,
    start: Instant,
    time_limit_ms: u64,
) -> bool {
    if state.is_solved(cb) {
        return true;
    }
    if depth_remaining == 0 {
        return false;
    }
    if (*expanded).is_multiple_of(4096) && start.elapsed().as_millis() as u64 >= time_limit_ms {
        return false;
    }

    *expanded += 1;

    let focus_mask = compute_focus_mask(cb, &state.box_cells, focus_radius);
    let successors = generate_successors_committed(cb, state, None, None, focus_mask);

    let mut scored: Vec<(u64, usize)> = Vec::with_capacity(successors.len());
    for (i, succ) in successors.iter().enumerate() {
        if deadlocks.is_deadlocked_quick(cb, &succ.state.box_cells) {
            continue;
        }
        let sh = quick_heuristic(cb, &succ.state.box_cells);
        if sh == u32::MAX || sh > h_ceiling {
            continue;
        }
        let succ_hash = succ.state.zobrist_hash(zk);
        if visited.contains(&succ_hash) {
            continue;
        }
        let sort_key = if noise_seed == 0 {
            (sh as u64) << 32
        } else {
            let noise = fnv_mix(succ_hash ^ noise_seed) & 0x3;
            ((sh as u64 + noise) << 32) | (succ_hash & 0xFFFF_FFFF)
        };
        scored.push((sort_key, i));
    }

    scored.sort_unstable_by_key(|&(k, _)| k);

    for (_, si) in scored {
        let succ = &successors[si];
        let succ_hash = succ.state.zobrist_hash(zk);

        path.push((succ.box_index, succ.direction));
        visited.insert(succ_hash);

        if dfs_recurse(
            cb,
            &succ.state,
            zk,
            deadlocks,
            focus_radius,
            h_ceiling,
            depth_remaining - 1,
            noise_seed,
            path,
            visited,
            expanded,
            start,
            time_limit_ms,
        ) {
            return true;
        }

        path.pop();
        visited.remove(&succ_hash);
    }

    false
}

fn reconstruct_path(
    closed: &rustc_hash::FxHashMap<u64, FocusedClosedEntry>,
    goal_hash: u64,
) -> Vec<(usize, Direction)> {
    let mut path = Vec::new();
    let mut current = goal_hash;

    while let Some(entry) = closed.get(&current) {
        if let (Some(dir), Some(bi)) = (entry.push_dir, entry.push_box_idx) {
            path.push((bi, dir));
        }
        match entry.parent_hash {
            Some(parent) => current = parent,
            None => break,
        }
    }

    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SolverLimits;
    use sokomind_core::board::parse_board;

    #[test]
    fn focused_endgame_trivial() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let deadlocks = DeadlockChecker::new(&cb);
        let limits = SolverLimits {
            max_time_ms: Some(5_000),
            max_expanded_states: Some(100_000),
            ..Default::default()
        };
        let mut budget = Budget::new(&limits);
        let mut counters = SearchCounters::default();
        let initial = DenseState::from_initial(&cb);

        match endgame_focused_search(
            &cb, &initial, &zk, &deadlocks,
            &mut budget, &mut counters, 4, 2,
        ) {
            EndgameResult::Solved { pushes } => {
                assert!(!pushes.is_empty());
            }
            EndgameResult::NotSolved => panic!("should solve trivial puzzle"),
        }
    }

    #[test]
    fn focused_endgame_2box() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let deadlocks = DeadlockChecker::new(&cb);
        let limits = SolverLimits {
            max_time_ms: Some(5_000),
            max_expanded_states: Some(100_000),
            ..Default::default()
        };
        let mut budget = Budget::new(&limits);
        let mut counters = SearchCounters::default();
        let initial = DenseState::from_initial(&cb);

        match endgame_focused_search(
            &cb, &initial, &zk, &deadlocks,
            &mut budget, &mut counters, 5, 2,
        ) {
            EndgameResult::Solved { pushes } => {
                assert!(pushes.len() >= 2);
            }
            EndgameResult::NotSolved => panic!("should solve 2-box puzzle"),
        }
    }

    #[test]
    fn greedy_dfs_trivial() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let deadlocks = DeadlockChecker::new(&cb);
        let initial = DenseState::from_initial(&cb);

        match endgame_greedy_dfs(&cb, &initial, &zk, &deadlocks, 4, 4, 20, 5_000, 0) {
            EndgameResult::Solved { pushes } => {
                assert!(!pushes.is_empty());
            }
            EndgameResult::NotSolved => panic!("should solve trivial puzzle"),
        }
    }

    #[test]
    fn greedy_dfs_2box() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let deadlocks = DeadlockChecker::new(&cb);
        let initial = DenseState::from_initial(&cb);

        match endgame_greedy_dfs(&cb, &initial, &zk, &deadlocks, 5, 6, 25, 5_000, 0) {
            EndgameResult::Solved { pushes } => {
                assert!(pushes.len() >= 2);
            }
            EndgameResult::NotSolved => panic!("should solve 2-box puzzle"),
        }
    }

    #[test]
    fn focus_mask_commits_distant_boxes() {
        let rows = &[
            "OOOOOOOOO",
            "O SSS   O",
            "O       O",
            "O XXX   O",
            "O   R   O",
            "O       O",
            "OOOOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let initial = DenseState::from_initial(&cb);

        let mask = compute_focus_mask(&cb, &initial.box_cells, 2);
        assert_eq!(mask, 0, "no on-goal boxes to commit when none are on goal");
    }
}
