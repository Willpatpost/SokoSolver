use serde::{Deserialize, Serialize};
use sokomind_core::game::GameSnapshot;
use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::cancellation::CancelToken;
use crate::compiled_board::CompiledBoard;
use crate::config::{SolverMode, SolverRequest};
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::heuristic::AssignmentHeuristic;
use crate::improvement;
use crate::log::PhaseLogger;
use crate::macros::MacroEngine;
use crate::planning::StructuralPlan;
use crate::reachability::find_keeper_path;
use crate::search::astar::{astar_search, AStarResult};
use crate::search::beam::{beam_search, BeamConfig, BeamResult};
use crate::search::ida_star::{ida_star_search, IDAStarResult};
use crate::verification::{verify_solution, VerificationError};
use crate::zobrist::ZobristKeys;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolverPhase {
    Preparing,
    Searching,
    Harvesting,
    Improving,
    Proving,
    Verifying,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolutionStep {
    pub direction: Direction,
    pub pushed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Solution {
    pub steps: Vec<SolutionStep>,
    pub moves: u32,
    pub pushes: u32,
    pub final_snapshot: GameSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverMetrics {
    pub elapsed_ms: f64,
    pub expanded_states: u64,
    pub generated_states: u64,
    pub peak_frontier: u64,
    pub peak_memory_bytes: usize,
    pub deadlock_prunes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverProof {
    pub kind: ProofKind,
    pub lower_bound: u32,
    pub upper_bound: u32,
    pub gap: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofKind {
    Bounded,
    Optimal,
    Unsolvable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SolverStatus {
    Solved,
    Unsolved { reason: String },
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverResult {
    pub status: SolverStatus,
    pub solution: Option<Solution>,
    pub metrics: SolverMetrics,
    pub proof: Option<SolverProof>,
    pub telemetry: crate::log::TelemetryCounters,
    pub phase_reports: Vec<crate::log::PhaseReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub phase: SolverPhase,
    pub elapsed_ms: f64,
    pub expanded_states: u64,
    pub generated_states: u64,
    pub best_pushes: Option<u32>,
    pub best_moves: Option<u32>,
    pub log_entries: Vec<crate::log::LogEntry>,
}

pub fn solve(request: &SolverRequest) -> SolverResult {
    solve_with_progress(request, CancelToken::new(), &mut |_| true)
}

pub fn solve_with_progress(
    request: &SolverRequest,
    cancel: CancelToken,
    on_progress: &mut dyn FnMut(&ProgressUpdate) -> bool,
) -> SolverResult {
    let mut logger = PhaseLogger::new(request.options.log_level);

    let fire_progress =
        |phase, budget: &Budget, counters: &SearchCounters, best: Option<u32>, logger: &mut PhaseLogger, cb: &mut dyn FnMut(&ProgressUpdate) -> bool| -> bool {
            cb(&ProgressUpdate {
                phase,
                elapsed_ms: budget.elapsed_ms(),
                expanded_states: counters.expanded,
                generated_states: counters.generated,
                best_pushes: best,
                best_moves: None,
                log_entries: logger.drain_entries(),
            })
        };

    // Phase 0: Prepare
    logger.start_phase(SolverPhase::Preparing);
    let cb = CompiledBoard::from_parsed(&request.board);
    let seed = request.options.seed;
    let zk = ZobristKeys::new(&cb, seed);
    let mut heuristic = AssignmentHeuristic::new(&cb);
    let deadlocks = DeadlockChecker::new(&cb);
    let macros = MacroEngine::new(&cb);
    let plan = StructuralPlan::build(&cb);
    let initial = DenseState::from_initial(&cb);
    let mut budget = Budget::with_cancel(&request.limits, cancel);
    let mut counters = SearchCounters::default();

    logger.set_counter("board.cells", cb.cell_count as f64);
    logger.set_counter("board.boxes", cb.initial_box_cells.len() as f64);
    logger.set_counter("board.goals", cb.goal_cells.len() as f64);
    logger.log(
        crate::config::LogLevel::Info,
        &format!(
            "board: {} cells, {} boxes, {} goals, mode={:?}",
            cb.cell_count,
            cb.initial_box_cells.len(),
            cb.goal_cells.len(),
            request.options.mode,
        ),
    );
    if plan.is_active() {
        logger.log(crate::config::LogLevel::Info, &plan.summary());
    }
    logger.end_phase();

    if !fire_progress(SolverPhase::Preparing, &budget, &counters, None, &mut logger, on_progress) {
        budget.cancel_handle().cancel();
    }

    if budget.is_cancelled() {
        return cancelled_result(&budget, &counters, logger);
    }

    // Phase 2/3: Search
    logger.start_phase(SolverPhase::Searching);
    let search_result = run_search(
        &cb,
        &initial,
        &zk,
        &mut heuristic,
        &deadlocks,
        &macros,
        &plan,
        &mut budget,
        &mut counters,
        &request.options.mode,
    );

    logger.set_counter("search.expanded", counters.expanded as f64);
    logger.set_counter("search.generated", counters.generated as f64);
    logger.set_counter("search.peak_frontier", counters.peak_frontier as f64);
    logger.set_counter("search.deadlock.static", counters.deadlock_static as f64);
    logger.set_counter("search.deadlock.two_by_two", counters.deadlock_two_by_two as f64);
    logger.set_counter("search.deadlock.freeze", counters.deadlock_freeze as f64);
    logger.set_counter("search.deadlock.pattern", counters.deadlock_pattern as f64);
    logger.set_counter("search.heuristic.calls", counters.heuristic_calls as f64);
    logger.set_counter("search.macro.forced_push", counters.macro_forced_push as f64);
    logger.set_counter("search.macro.tunnel", counters.macro_tunnel as f64);
    logger.set_counter("search.transposition.unique", counters.transposition_unique as f64);
    logger.set_counter("search.transposition.duplicate", counters.transposition_duplicate as f64);

    let outcome_str = match &search_result {
        SearchOutcome::Solved { pushes, optimal } => {
            format!("found solution ({} pushes, optimal={})", pushes.len(), optimal)
        }
        SearchOutcome::Exhausted => "search space exhausted".into(),
        SearchOutcome::BudgetExceeded => "budget exceeded".into(),
    };
    logger.log(
        crate::config::LogLevel::Info,
        &format!(
            "search: {} — expanded={}, generated={}, deadlock_prunes={}",
            outcome_str,
            counters.expanded,
            counters.generated,
            counters.deadlock_static + counters.deadlock_two_by_two
                + counters.deadlock_freeze + counters.deadlock_pattern
                + counters.deadlock_pi_corral + counters.deadlock_table,
        ),
    );
    logger.end_phase();

    let best_pushes = match &search_result {
        SearchOutcome::Solved { pushes, .. } => Some(pushes.len() as u32),
        _ => None,
    };
    if !fire_progress(SolverPhase::Searching, &budget, &counters, best_pushes, &mut logger, on_progress) {
        budget.cancel_handle().cancel();
    }

    if budget.is_cancelled() && best_pushes.is_none() {
        return cancelled_result(&budget, &counters, logger);
    }

    let total_deadlock_prunes = counters.deadlock_static
        + counters.deadlock_two_by_two
        + counters.deadlock_freeze
        + counters.deadlock_pattern
        + counters.deadlock_pi_corral
        + counters.deadlock_table;

    let metrics = SolverMetrics {
        elapsed_ms: budget.elapsed_ms(),
        expanded_states: counters.expanded,
        generated_states: counters.generated,
        peak_frontier: counters.peak_frontier,
        peak_memory_bytes: 0,
        deadlock_prunes: total_deadlock_prunes,
    };

    let telemetry = counters.to_telemetry(0);

    match search_result {
        SearchOutcome::Solved {
            mut pushes,
            optimal,
        } => {
            if !optimal {
                let pre_pushes = pushes.len();
                logger.start_phase(SolverPhase::Improving);
                let imp_report = improvement::improve_push_sequence(&cb, &deadlocks, &mut pushes);
                logger.set_counter("improvement.pushes_saved", imp_report.pushes_saved as f64);
                logger.set_counter("improvement.moves_saved", imp_report.moves_saved as f64);
                logger.log(
                    crate::config::LogLevel::Info,
                    &format!(
                        "improvement: {} → {} pushes (saved {}p, {}m)",
                        pre_pushes,
                        pushes.len(),
                        imp_report.pushes_saved,
                        imp_report.moves_saved,
                    ),
                );
                logger.end_phase();
                fire_progress(SolverPhase::Improving, &budget, &counters, Some(pushes.len() as u32), &mut logger, on_progress);
            }

            logger.start_phase(SolverPhase::Verifying);
            match pushes_to_solution(&cb, &request.board, &pushes) {
                Ok(solution) => {
                    logger.set_counter("solution.moves", solution.moves as f64);
                    logger.set_counter("solution.pushes", solution.pushes as f64);
                    logger.log(
                        crate::config::LogLevel::Info,
                        &format!(
                            "verified: {} moves, {} pushes",
                            solution.moves, solution.pushes,
                        ),
                    );
                    logger.end_phase();
                    let proof = if optimal {
                        Some(crate::proof::optimal_proof(solution.pushes))
                    } else {
                        Some(crate::proof::bounded_proof(0, solution.pushes))
                    };
                    SolverResult {
                        status: SolverStatus::Solved,
                        solution: Some(solution),
                        metrics,
                        proof,
                        telemetry: telemetry.clone(),
                        phase_reports: logger.into_reports(),
                    }
                }
                Err(e) => {
                    logger.end_phase();
                    SolverResult {
                        status: SolverStatus::Unsolved {
                            reason: format!("solution verification failed: {}", e),
                        },
                        solution: None,
                        metrics,
                        proof: None,
                        telemetry: telemetry.clone(),
                        phase_reports: logger.into_reports(),
                    }
                }
            }
        }
        SearchOutcome::Exhausted => SolverResult {
            status: SolverStatus::Unsolved {
                reason: "search space exhausted — no solution exists".into(),
            },
            solution: None,
            metrics,
            proof: Some(crate::proof::unsolvable_proof()),
            telemetry: telemetry.clone(),
            phase_reports: logger.into_reports(),
        },
        SearchOutcome::BudgetExceeded => SolverResult {
            status: if budget.is_cancelled() {
                SolverStatus::Cancelled
            } else {
                SolverStatus::Unsolved {
                    reason: "budget exceeded".into(),
                }
            },
            solution: None,
            metrics,
            proof: None,
            telemetry,
            phase_reports: logger.into_reports(),
        },
    }
}

fn cancelled_result(
    budget: &Budget,
    counters: &SearchCounters,
    logger: PhaseLogger,
) -> SolverResult {
    let total_deadlock_prunes = counters.deadlock_static
        + counters.deadlock_two_by_two
        + counters.deadlock_freeze
        + counters.deadlock_pattern
        + counters.deadlock_pi_corral
        + counters.deadlock_table;

    SolverResult {
        status: SolverStatus::Cancelled,
        solution: None,
        metrics: SolverMetrics {
            elapsed_ms: budget.elapsed_ms(),
            expanded_states: counters.expanded,
            generated_states: counters.generated,
            peak_frontier: counters.peak_frontier,
            peak_memory_bytes: 0,
            deadlock_prunes: total_deadlock_prunes,
        },
        proof: None,
        telemetry: counters.to_telemetry(0),
        phase_reports: logger.into_reports(),
    }
}

enum SearchOutcome {
    Solved {
        pushes: Vec<(usize, Direction)>,
        optimal: bool,
    },
    Exhausted,
    BudgetExceeded,
}

fn is_small_puzzle(cb: &CompiledBoard) -> bool {
    cb.initial_box_cells.len() <= 5
}

/// Adaptive search strategy:
/// - Small puzzles (<=5 boxes): A* first (fast, proves optimality), beam fallback
/// - Larger puzzles: beam search first (bounded memory), A* fallback
/// - Quality/Optimal: proof attempt with A*/IDA* after discovery
#[allow(clippy::too_many_arguments)]
fn run_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    macros: &MacroEngine,
    plan: &StructuralPlan,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    mode: &SolverMode,
) -> SearchOutcome {
    if is_small_puzzle(cb) {
        run_small_puzzle_search(
            cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, mode,
        )
    } else {
        run_large_puzzle_search(
            cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, mode,
        )
    }
}

/// Small puzzle: A* first → beam fallback → IDA* proof (quality/optimal)
#[allow(clippy::too_many_arguments)]
fn run_small_puzzle_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    macros: &MacroEngine,
    plan: &StructuralPlan,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    mode: &SolverMode,
) -> SearchOutcome {
    let astar_result = astar_search(cb, initial, zk, heuristic, deadlocks, budget, counters);
    match astar_result {
        AStarResult::Solved { pushes, .. } => SearchOutcome::Solved {
            pushes,
            optimal: true,
        },
        AStarResult::Exhausted => SearchOutcome::Exhausted,
        AStarResult::BudgetExceeded => {
            heuristic.clear_cache();
            let beam_result = try_beam(
                cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters,
            );
            if let Some(outcome) = beam_result {
                return outcome;
            }
            if matches!(mode, SolverMode::Quality | SolverMode::Optimal) && !budget.exhausted() {
                heuristic.clear_cache();
                try_ida_star(
                    cb, initial, zk, heuristic, deadlocks, budget, counters, None,
                )
            } else {
                SearchOutcome::BudgetExceeded
            }
        }
    }
}

/// Large puzzle: beam first → A* fallback → IDA* proof (quality/optimal)
#[allow(clippy::too_many_arguments)]
fn run_large_puzzle_search(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    macros: &MacroEngine,
    plan: &StructuralPlan,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    mode: &SolverMode,
) -> SearchOutcome {
    let beam_result = try_beam(
        cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters,
    );
    if let Some(outcome) = beam_result {
        if let SearchOutcome::Solved { ref pushes, .. } = outcome {
            if matches!(mode, SolverMode::Quality | SolverMode::Optimal) && !budget.exhausted() {
                let upper = pushes.len() as u32;
                heuristic.clear_cache();
                let proof = try_ida_star(
                    cb,
                    initial,
                    zk,
                    heuristic,
                    deadlocks,
                    budget,
                    counters,
                    Some(upper),
                );
                if let SearchOutcome::Solved {
                    pushes: proof_pushes,
                    ..
                } = proof
                {
                    return SearchOutcome::Solved {
                        pushes: proof_pushes,
                        optimal: true,
                    };
                }
            }
        }
        return outcome;
    }

    if !budget.exhausted() {
        heuristic.clear_cache();
        let astar_result = astar_search(cb, initial, zk, heuristic, deadlocks, budget, counters);
        match astar_result {
            AStarResult::Solved { pushes, .. } => SearchOutcome::Solved {
                pushes,
                optimal: true,
            },
            AStarResult::Exhausted => SearchOutcome::Exhausted,
            AStarResult::BudgetExceeded => SearchOutcome::BudgetExceeded,
        }
    } else {
        SearchOutcome::BudgetExceeded
    }
}

#[allow(clippy::too_many_arguments)]
fn try_beam(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    macros: &MacroEngine,
    plan: &StructuralPlan,
    budget: &mut Budget,
    counters: &mut SearchCounters,
) -> Option<SearchOutcome> {
    let config = BeamConfig::default();
    let plan_ref = if plan.is_active() { Some(plan) } else { None };
    match beam_search(
        cb, initial, zk, heuristic, deadlocks, macros, budget, counters, &config, plan_ref,
    ) {
        BeamResult::Solved { mut incumbents } => {
            incumbents.sort_by_key(|inc| inc.push_count);
            let best = incumbents.into_iter().next().unwrap();
            Some(SearchOutcome::Solved {
                pushes: best.pushes,
                optimal: false,
            })
        }
        BeamResult::NoSolution => None,
        BeamResult::BudgetExceeded => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn try_ida_star(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    upper_bound: Option<u32>,
) -> SearchOutcome {
    match ida_star_search(
        cb,
        initial,
        zk,
        heuristic,
        deadlocks,
        budget,
        counters,
        upper_bound,
    ) {
        IDAStarResult::Solved { pushes, .. } => SearchOutcome::Solved {
            pushes,
            optimal: true,
        },
        IDAStarResult::Exhausted => SearchOutcome::Exhausted,
        IDAStarResult::BudgetExceeded => SearchOutcome::BudgetExceeded,
    }
}

/// Convert a push sequence from the search into a full move sequence with
/// walk steps, then verify through the core game engine.
fn pushes_to_solution(
    cb: &CompiledBoard,
    board: &sokomind_core::board::ParsedBoard,
    pushes: &[(usize, Direction)],
) -> Result<Solution, PushConversionError> {
    let mut steps: Vec<(Direction, bool)> = Vec::new();
    let mut keeper = cb.robot_cell;
    let mut box_cells: Vec<(u16, u8)> = cb
        .initial_box_cells
        .iter()
        .map(|&(c, l)| (c, l.0))
        .collect();
    box_cells.sort();

    for &(box_index, push_dir) in pushes {
        let box_cell = box_cells[box_index].0;
        let push_from = cb.neighbor(box_cell, push_dir.opposite());

        let walk_path = find_keeper_path(cb, keeper, push_from, &box_cells)
            .ok_or(PushConversionError::UnreachablePushPosition)?;

        for &dir in &walk_path {
            steps.push((dir, false));
        }

        steps.push((push_dir, true));

        let target = cb.neighbor(box_cell, push_dir);
        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
        keeper = box_cell;
    }

    improvement::move_window::optimize_walks(cb, &mut steps);

    let (moves, push_count) =
        verify_solution(board, &steps).map_err(PushConversionError::Verification)?;

    let snapshot = sokomind_core::game::create_snapshot(board);
    let mut current = snapshot;
    for &(dir, _) in &steps {
        let t = sokomind_core::game::step_snapshot(board, &current, dir);
        current = t.snapshot;
    }

    Ok(Solution {
        steps: steps
            .into_iter()
            .map(|(direction, pushed)| SolutionStep { direction, pushed })
            .collect(),
        moves,
        pushes: push_count,
        final_snapshot: current,
    })
}

#[derive(Debug)]
enum PushConversionError {
    UnreachablePushPosition,
    Verification(VerificationError),
}

impl std::fmt::Display for PushConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PushConversionError::UnreachablePushPosition => {
                write!(f, "keeper cannot reach push position")
            }
            PushConversionError::Verification(e) => write!(f, "{}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LogLevel, SolverLimits, SolverOptions};
    use sokomind_core::board::parse_board;
    use sokomind_core::game::create_snapshot;

    fn make_request(rows: &[&str]) -> SolverRequest {
        let board = parse_board(rows).unwrap();
        let snapshot = create_snapshot(&board);
        SolverRequest {
            board,
            snapshot,
            limits: SolverLimits {
                max_time_ms: Some(10_000),
                max_expanded_states: Some(500_000),
                max_generated_states: None,
                max_memory_bytes: None,
            },
            options: SolverOptions {
                mode: SolverMode::Fast,
                deterministic: true,
                log_level: LogLevel::Info,
                seed: Some(42),
            },
        }
    }

    #[test]
    fn solve_trivial_1box() {
        let request = make_request(&["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"]);
        let result = solve(&request);
        match result.status {
            SolverStatus::Solved => {
                let sol = result.solution.unwrap();
                assert!(sol.pushes >= 2);
                assert!(sol.final_snapshot.solved);
            }
            other => panic!("expected Solved, got {:?}", other),
        }
    }

    #[test]
    fn solve_2box() {
        let request = make_request(&[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ]);
        let result = solve(&request);
        match result.status {
            SolverStatus::Solved => {
                let sol = result.solution.unwrap();
                assert!(sol.pushes >= 2);
                assert!(sol.final_snapshot.solved);
            }
            other => panic!("expected Solved, got {:?}", other),
        }
    }

    #[test]
    fn solve_typed_labels() {
        let request = make_request(&[
            "OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO",
        ]);
        let result = solve(&request);
        match result.status {
            SolverStatus::Solved => {
                let sol = result.solution.unwrap();
                assert!(sol.pushes >= 2);
                assert!(sol.final_snapshot.solved);
            }
            other => panic!("expected Solved, got {:?}", other),
        }
    }

    #[test]
    fn unsolvable_reports_exhausted() {
        let request = make_request(&["OOOOO", "OX  O", "O  SO", "O  RO", "OOOOO"]);
        let result = solve(&request);
        match result.status {
            SolverStatus::Unsolved { .. } => {
                assert!(result.proof.is_some());
            }
            SolverStatus::Solved => panic!("should not solve"),
            _ => {}
        }
    }

    #[test]
    fn metrics_populated() {
        let request = make_request(&["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"]);
        let result = solve(&request);
        assert!(result.metrics.expanded_states > 0);
        assert!(result.metrics.generated_states > 0);
    }

    #[test]
    fn solution_steps_verified() {
        let request = make_request(&["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"]);
        let result = solve(&request);
        if let SolverStatus::Solved = result.status {
            let sol = result.solution.unwrap();
            let steps: Vec<(Direction, bool)> =
                sol.steps.iter().map(|s| (s.direction, s.pushed)).collect();
            let verified = verify_solution(&request.board, &steps);
            assert!(verified.is_ok());
        }
    }

    #[test]
    fn solve_6box_uses_beam_path() {
        // 6 boxes triggers the large-puzzle (beam-first) path
        // Narrow layout keeps search space manageable in debug builds
        let request = make_request(&[
            "OOOOOOO", "OSX R O", "OSX   O", "OSX   O", "OSX   O", "OSX   O", "OSX   O", "OOOOOOO",
        ]);
        let result = solve(&request);
        match result.status {
            SolverStatus::Solved => {
                let sol = result.solution.unwrap();
                assert!(sol.pushes >= 6);
                assert!(sol.final_snapshot.solved);
                let steps: Vec<(Direction, bool)> =
                    sol.steps.iter().map(|s| (s.direction, s.pushed)).collect();
                let verified = verify_solution(&request.board, &steps);
                assert!(verified.is_ok());
            }
            other => panic!("expected Solved for 6-box puzzle, got {:?}", other),
        }
    }

    #[test]
    fn solve_3box_via_astar_path() {
        // 3 boxes: small puzzle, takes the A* path
        let request = make_request(&[
            "OOOOOOOOO",
            "O SSS   O",
            "O       O",
            "O XXX   O",
            "O   R   O",
            "O       O",
            "OOOOOOOOO",
        ]);
        let result = solve(&request);
        match result.status {
            SolverStatus::Solved => {
                let sol = result.solution.unwrap();
                assert!(sol.pushes >= 3);
                assert!(sol.final_snapshot.solved);
            }
            other => panic!("expected Solved for 3-box puzzle, got {:?}", other),
        }
    }
}
