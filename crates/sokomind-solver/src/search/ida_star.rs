use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::heuristic::AssignmentHeuristic;
use crate::zobrist::ZobristKeys;

use super::successors::generate_successors;

pub enum IDAStarResult {
    Solved {
        pushes: Vec<(usize, Direction)>,
        final_state: DenseState,
    },
    Exhausted,
    BudgetExceeded,
}

/// Move-optimal iterative deepening A* search.
/// Memory-efficient alternative to A* for proof mode on larger puzzles.
/// g-cost = total moves (walks + pushes). Uses f-cost threshold that
/// increases each iteration.
#[allow(clippy::too_many_arguments)]
pub fn ida_star_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    upper_bound: Option<u32>,
) -> IDAStarResult {
    let init_hash = initial.zobrist_hash_exact(zk);
    let h = heuristic.evaluate(cb, initial, zk.hash_boxes(&initial.box_cells));
    counters.heuristic_calls += 1;

    if h == u32::MAX {
        return IDAStarResult::Exhausted;
    }

    let mut threshold = h;
    let max_threshold = upper_bound.unwrap_or(u32::MAX);

    loop {
        if threshold > max_threshold {
            return IDAStarResult::Exhausted;
        }

        let mut path = vec![(initial.clone(), init_hash)];
        let mut push_path: Vec<(usize, Direction)> = Vec::new();

        let result = dfs(
            cb,
            zk,
            heuristic,
            deadlocks,
            budget,
            counters,
            &mut path,
            &mut push_path,
            0,
            threshold,
        );

        match result {
            DfsResult::Found => {
                let final_state = path.last().unwrap().0.clone();
                return IDAStarResult::Solved {
                    pushes: push_path,
                    final_state,
                };
            }
            DfsResult::BudgetExceeded => return IDAStarResult::BudgetExceeded,
            DfsResult::NotFound { next_threshold } => {
                if next_threshold == u32::MAX {
                    return IDAStarResult::Exhausted;
                }
                threshold = next_threshold;
            }
        }
    }
}

enum DfsResult {
    Found,
    NotFound { next_threshold: u32 },
    BudgetExceeded,
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    cb: &CompiledBoard,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    path: &mut Vec<(DenseState, u64)>,
    push_path: &mut Vec<(usize, Direction)>,
    g: u32,
    threshold: u32,
) -> DfsResult {
    if budget.exhausted() {
        return DfsResult::BudgetExceeded;
    }

    let (current, _current_hash) = path.last().unwrap();

    let h = heuristic.evaluate(cb, current, zk.hash_boxes(&current.box_cells));
    counters.heuristic_calls += 1;
    let f = g.saturating_add(h);

    if f > threshold {
        return DfsResult::NotFound { next_threshold: f };
    }

    budget.tick_expanded();
    counters.expanded += 1;

    if current.is_solved(cb) {
        return DfsResult::Found;
    }

    let successors = generate_successors(cb, current);
    counters.generated += successors.len() as u64;
    budget.tick_generated(successors.len() as u64);

    let mut min_next = u32::MAX;

    for succ in successors {
        if deadlocks.is_deadlocked(cb, succ.state.keeper_zone, &succ.state.box_cells, counters) {
            continue;
        }

        let succ_hash = succ.state.zobrist_hash_exact(zk);

        if path.iter().any(|(_, h)| *h == succ_hash) {
            continue;
        }

        let child_g = g + succ.walk_cost + succ.push_count;
        path.push((succ.state, succ_hash));
        push_path.push((succ.box_index, succ.direction));

        let result = dfs(
            cb,
            zk,
            heuristic,
            deadlocks,
            budget,
            counters,
            path,
            push_path,
            child_g,
            threshold,
        );

        match result {
            DfsResult::Found => return DfsResult::Found,
            DfsResult::BudgetExceeded => return DfsResult::BudgetExceeded,
            DfsResult::NotFound { next_threshold } => {
                if next_threshold < min_next {
                    min_next = next_threshold;
                }
            }
        }

        path.pop();
        push_path.pop();
    }

    DfsResult::NotFound {
        next_threshold: min_next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SolverLimits;
    use sokomind_core::board::parse_board;

    fn solve_puzzle(rows: &[&str]) -> IDAStarResult {
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let deadlocks = DeadlockChecker::new(&cb);
        let limits = SolverLimits {
            max_time_ms: Some(10_000),
            max_expanded_states: Some(500_000),
            max_generated_states: None,
            max_memory_bytes: None,
        };
        let mut budget = Budget::new(&limits);
        let mut counters = SearchCounters::default();
        let initial = DenseState::from_initial(&cb);

        ida_star_search(
            &cb,
            &initial,
            &zk,
            &mut heuristic,
            &deadlocks,
            &mut budget,
            &mut counters,
            None,
        )
    }

    #[test]
    fn ida_solve_trivial() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        match solve_puzzle(rows) {
            IDAStarResult::Solved { pushes, .. } => {
                assert!(!pushes.is_empty());
            }
            _ => panic!("expected Solved"),
        }
    }

    #[test]
    fn ida_finds_optimal() {
        // Simple puzzle where optimal is known: 2 pushes
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        match solve_puzzle(rows) {
            IDAStarResult::Solved { pushes, .. } => {
                assert_eq!(pushes.len(), 2, "optimal solution is 2 pushes");
            }
            _ => panic!("expected Solved"),
        }
    }

    #[test]
    fn ida_unsolvable() {
        let rows = &["OOOOO", "OX  O", "O  SO", "O  RO", "OOOOO"];
        match solve_puzzle(rows) {
            IDAStarResult::Exhausted => {}
            IDAStarResult::BudgetExceeded => {}
            IDAStarResult::Solved { .. } => panic!("should not solve"),
        }
    }
}
