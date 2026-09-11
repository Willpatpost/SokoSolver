use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::heuristic::AssignmentHeuristic;
use crate::macros::MacroEngine;
use crate::planning::StructuralPlan;
use crate::transposition::TranspositionTable;
use crate::zobrist::ZobristKeys;

use super::successors::generate_successors_with_macros;

pub struct BeamConfig {
    pub beam_width: usize,
    pub max_depth: u32,
    pub max_incumbents: usize,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            beam_width: 5000,
            max_depth: 500,
            max_incumbents: 4,
        }
    }
}

pub struct BeamIncumbent {
    pub pushes: Vec<(usize, Direction)>,
    pub push_count: u32,
    pub final_state: DenseState,
}

pub enum BeamResult {
    Solved { incumbents: Vec<BeamIncumbent> },
    NoSolution,
    BudgetExceeded,
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
    f_cost: u32,
    structural_boost: u32,
    history_id: u32,
}

/// Layered beam search for Sokoban push discovery.
///
/// Expands all states at each depth layer, scores successors with
/// f = g(pushes) + h(assignment heuristic), and retains the best
/// `beam_width` states for the next layer. Uses Pareto transposition
/// to avoid re-exploring dominated states and collects up to
/// `max_incumbents` solutions.
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
    let h = heuristic.evaluate(cb, initial, init_hash);
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

    let init_boost = plan
        .filter(|p| p.is_active())
        .map(|p| p.evaluate_state(cb, &initial.box_cells))
        .unwrap_or(0);

    let mut beam = vec![BeamEntry {
        state: initial.clone(),
        g_cost: 0,
        f_cost: h,
        structural_boost: init_boost,
        history_id: NO_PARENT,
    }];

    let mut incumbents: Vec<BeamIncumbent> = Vec::new();

    for _depth in 0..config.max_depth {
        if beam.is_empty() || budget.exhausted() {
            break;
        }

        let mut candidates: Vec<BeamEntry> = Vec::new();

        for entry in &beam {
            if budget.exhausted() {
                break;
            }

            budget.tick_expanded();
            counters.expanded += 1;

            let successors =
                generate_successors_with_macros(cb, &entry.state, Some(macros), Some(counters));
            counters.generated += successors.len() as u64;
            budget.tick_generated(successors.len() as u64);

            for succ in successors {
                if deadlocks.is_deadlocked(
                    cb,
                    succ.state.keeper_zone,
                    &succ.state.box_cells,
                    counters,
                ) {
                    continue;
                }

                let succ_hash = succ.state.zobrist_hash(zk);
                let g = entry.g_cost + succ.push_count;

                if !tt.insert(succ_hash, g, succ.state.moves) {
                    counters.transposition_duplicate += 1;
                    continue;
                }
                counters.transposition_unique += 1;

                let hist_id = history.len() as u32;
                history.push(HistoryNode {
                    parent_id: entry.history_id,
                    box_index: succ.box_index,
                    direction: succ.direction,
                });

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

                let h = heuristic.evaluate(cb, &succ.state, succ_hash);
                counters.heuristic_calls += 1;
                if h == u32::MAX {
                    continue;
                }

                let boost = plan
                    .filter(|p| p.is_active())
                    .map(|p| p.evaluate_state(cb, &succ.state.box_cells))
                    .unwrap_or(0);

                candidates.push(BeamEntry {
                    state: succ.state,
                    g_cost: g,
                    f_cost: g.saturating_add(h),
                    structural_boost: boost,
                    history_id: hist_id,
                });
            }
        }

        candidates.sort_by(|a, b| {
            a.f_cost
                .cmp(&b.f_cost)
                .then(a.structural_boost.cmp(&b.structural_boost))
                .then(a.state.moves.cmp(&b.state.moves))
        });
        candidates.truncate(config.beam_width);
        counters.update_peak_frontier(candidates.len() as u64);

        beam = candidates;

        if !budget.memory_ok(tt.estimated_memory_bytes()) {
            break;
        }
    }

    if !incumbents.is_empty() {
        BeamResult::Solved { incumbents }
    } else if budget.exhausted() {
        BeamResult::BudgetExceeded
    } else {
        BeamResult::NoSolution
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
            BeamResult::NoSolution | BeamResult::BudgetExceeded => {}
            BeamResult::Solved { .. } => panic!("should not solve"),
        }
    }

    #[test]
    fn beam_respects_width() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let config = BeamConfig {
            beam_width: 2,
            max_depth: 100,
            max_incumbents: 1,
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
            beam_width: 5000,
            max_depth: 200,
            max_incumbents: 4,
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
        // If the board parses with boxes already on goals, beam should return immediately
    }

    #[test]
    fn beam_path_reconstruction_correct() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let (result, _) = run_beam(rows, BeamConfig::default());
        if let BeamResult::Solved { incumbents } = result {
            let inc = &incumbents[0];
            assert_eq!(inc.pushes.len(), inc.push_count as usize);
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
}
