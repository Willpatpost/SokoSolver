use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::heuristic::AssignmentHeuristic;
use crate::priority_queue::MinPriorityQueue;
use crate::transposition::TranspositionTable;
use crate::zobrist::ZobristKeys;

use super::successors::generate_successors;

/// Search node stored in the open set.
struct AStarNode {
    state: DenseState,
    parent_hash: Option<u64>,
    push_dir: Option<Direction>,
    push_box_idx: Option<usize>,
    g_cost: u32,
    hash: u64,
}

/// Closed-set entry for path reconstruction.
struct ClosedEntry {
    parent_hash: Option<u64>,
    push_dir: Option<Direction>,
    push_box_idx: Option<usize>,
}

/// Result of an A* search.
pub enum AStarResult {
    Solved {
        pushes: Vec<(usize, Direction)>,
        final_state: DenseState,
    },
    Exhausted,
    BudgetExceeded,
}

/// Push-optimal A* search.
/// Uses assignment heuristic as admissible lower bound on remaining pushes.
/// g-cost = pushes so far, h-cost = heuristic estimate.
pub fn astar_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    budget: &mut Budget,
    counters: &mut SearchCounters,
) -> AStarResult {
    let init_hash = initial.zobrist_hash(zk);
    let h = heuristic.evaluate(cb, initial, init_hash);
    counters.heuristic_calls += 1;

    if h == u32::MAX {
        return AStarResult::Exhausted;
    }

    let mut open = MinPriorityQueue::new();
    let mut tt = TranspositionTable::new();
    let mut closed = rustc_hash::FxHashMap::default();

    tt.insert(init_hash, 0, 0);
    open.push(
        h,
        AStarNode {
            state: initial.clone(),
            parent_hash: None,
            push_dir: None,
            push_box_idx: None,
            g_cost: 0,
            hash: init_hash,
        },
    );

    while let Some((_, node)) = open.pop() {
        if budget.exhausted() {
            return AStarResult::BudgetExceeded;
        }

        if let Some(entry) = tt.get(node.hash) {
            if node.g_cost > entry.pushes {
                counters.duplicates += 1;
                continue;
            }
        }

        budget.tick_expanded();
        counters.expanded += 1;

        closed.insert(
            node.hash,
            ClosedEntry {
                parent_hash: node.parent_hash,
                push_dir: node.push_dir,
                push_box_idx: node.push_box_idx,
            },
        );

        if node.state.is_solved(cb) {
            let path = reconstruct_path(&closed, node.hash);
            return AStarResult::Solved {
                pushes: path,
                final_state: node.state,
            };
        }

        let successors = generate_successors(cb, &node.state);
        counters.generated += successors.len() as u64;
        budget.tick_generated(successors.len() as u64);

        for succ in successors {
            if deadlocks.is_deadlocked(cb, succ.state.keeper_zone, &succ.state.box_cells, counters)
            {
                continue;
            }

            let succ_hash = succ.state.zobrist_hash(zk);
            let g = node.g_cost + 1;

            if !tt.insert(succ_hash, g, succ.state.moves) {
                counters.transposition_duplicate += 1;
                continue;
            }
            counters.transposition_unique += 1;

            let h = heuristic.evaluate(cb, &succ.state, succ_hash);
            counters.heuristic_calls += 1;
            if h == u32::MAX {
                continue;
            }

            let f = g.saturating_add(h);

            open.push(
                f,
                AStarNode {
                    state: succ.state,
                    parent_hash: Some(node.hash),
                    push_dir: Some(succ.direction),
                    push_box_idx: Some(succ.box_index),
                    g_cost: g,
                    hash: succ_hash,
                },
            );
        }

        counters.update_peak_frontier(open.len() as u64);

        if !budget.memory_ok(tt.estimated_memory_bytes()) {
            return AStarResult::BudgetExceeded;
        }
    }

    AStarResult::Exhausted
}

fn reconstruct_path(
    closed: &rustc_hash::FxHashMap<u64, ClosedEntry>,
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

    fn solve_puzzle(rows: &[&str]) -> AStarResult {
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let deadlocks = DeadlockChecker::new(&cb);
        let limits = SolverLimits {
            max_time_ms: Some(10_000),
            max_expanded_states: Some(100_000),
            max_generated_states: None,
            max_memory_bytes: None,
        };
        let mut budget = Budget::new(&limits);
        let mut counters = SearchCounters::default();
        let initial = DenseState::from_initial(&cb);

        astar_search(
            &cb,
            &initial,
            &zk,
            &mut heuristic,
            &deadlocks,
            &mut budget,
            &mut counters,
        )
    }

    #[test]
    fn solve_trivial_1box() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        match solve_puzzle(rows) {
            AStarResult::Solved { pushes, .. } => {
                assert!(!pushes.is_empty());
            }
            other => panic!(
                "expected Solved, got {:?}",
                match other {
                    AStarResult::Exhausted => "Exhausted",
                    AStarResult::BudgetExceeded => "BudgetExceeded",
                    _ => "?",
                }
            ),
        }
    }

    #[test]
    fn solve_2box() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        match solve_puzzle(rows) {
            AStarResult::Solved { pushes, .. } => {
                assert!(pushes.len() >= 2);
            }
            other => panic!(
                "expected Solved, got {:?}",
                match other {
                    AStarResult::Exhausted => "Exhausted",
                    AStarResult::BudgetExceeded => "BudgetExceeded",
                    _ => "?",
                }
            ),
        }
    }

    #[test]
    fn solve_typed_labels() {
        let rows = &[
            "OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO",
        ];
        match solve_puzzle(rows) {
            AStarResult::Solved { pushes, .. } => {
                assert!(pushes.len() >= 2);
            }
            other => panic!(
                "expected Solved, got {:?}",
                match other {
                    AStarResult::Exhausted => "Exhausted",
                    AStarResult::BudgetExceeded => "BudgetExceeded",
                    _ => "?",
                }
            ),
        }
    }

    #[test]
    fn unsolvable_returns_exhausted() {
        // Box trapped in a corner — deadlock, no solution
        let rows = &["OOOOO", "OX  O", "O  SO", "O  RO", "OOOOO"];
        match solve_puzzle(rows) {
            AStarResult::Exhausted => {}
            AStarResult::Solved { .. } => panic!("should not solve"),
            AStarResult::BudgetExceeded => {} // also acceptable
        }
    }
}
