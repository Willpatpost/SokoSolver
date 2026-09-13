use serde::{Deserialize, Serialize};
use sokomind_core::game::GameSnapshot;
use sokomind_core::position::Direction;

use crate::budget::Budget;
use crate::cancellation::CancelToken;
use crate::compiled_board::CompiledBoard;
use crate::config::{LogLevel, SolverMode, SolverRequest};
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::heuristic::AssignmentHeuristic;
use crate::improvement;
use crate::log::PhaseLogger;
use crate::macros::MacroEngine;
use crate::planning::StructuralPlan;
use crate::reachability::find_keeper_path;
use crate::search::astar::{astar_search, astar_search_quick, AStarResult};
use crate::search::beam::{beam_search, goal_packing_count, sum_min_distances, BeamConfig, BeamResult};
use crate::search::endgame_dfs::{endgame_focused_search, endgame_greedy_dfs, EndgameResult};
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

    let fire_progress = |phase,
                         budget: &Budget,
                         counters: &SearchCounters,
                         best: Option<u32>,
                         logger: &mut PhaseLogger,
                         cb: &mut dyn FnMut(&ProgressUpdate) -> bool|
     -> bool {
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

    if !fire_progress(
        SolverPhase::Preparing,
        &budget,
        &counters,
        None,
        &mut logger,
        on_progress,
    ) {
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
        &mut logger,
    );

    logger.set_counter("search.expanded", counters.expanded as f64);
    logger.set_counter("search.generated", counters.generated as f64);
    logger.set_counter("search.peak_frontier", counters.peak_frontier as f64);
    logger.set_counter("search.deadlock.static", counters.deadlock_static as f64);
    logger.set_counter(
        "search.deadlock.two_by_two",
        counters.deadlock_two_by_two as f64,
    );
    logger.set_counter("search.deadlock.freeze", counters.deadlock_freeze as f64);
    logger.set_counter("search.deadlock.pattern", counters.deadlock_pattern as f64);
    logger.set_counter("search.heuristic.calls", counters.heuristic_calls as f64);
    logger.set_counter(
        "search.macro.forced_push",
        counters.macro_forced_push as f64,
    );
    logger.set_counter("search.macro.tunnel", counters.macro_tunnel as f64);
    logger.set_counter(
        "search.transposition.unique",
        counters.transposition_unique as f64,
    );
    logger.set_counter(
        "search.transposition.duplicate",
        counters.transposition_duplicate as f64,
    );

    let outcome_str = match &search_result {
        SearchOutcome::Solved { pushes, optimal } => {
            format!(
                "found solution ({} pushes, optimal={})",
                pushes.len(),
                optimal
            )
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
            counters.deadlock_static
                + counters.deadlock_two_by_two
                + counters.deadlock_freeze
                + counters.deadlock_pattern
                + counters.deadlock_goal_commitment
                + counters.deadlock_pi_corral
                + counters.deadlock_table,
        ),
    );
    logger.end_phase();

    let best_pushes = match &search_result {
        SearchOutcome::Solved { pushes, .. } => Some(pushes.len() as u32),
        _ => None,
    };
    if !fire_progress(
        SolverPhase::Searching,
        &budget,
        &counters,
        best_pushes,
        &mut logger,
        on_progress,
    ) {
        budget.cancel_handle().cancel();
    }

    if budget.is_cancelled() && best_pushes.is_none() {
        return cancelled_result(&budget, &counters, logger);
    }

    let total_deadlock_prunes = counters.deadlock_static
        + counters.deadlock_two_by_two
        + counters.deadlock_freeze
        + counters.deadlock_pattern
        + counters.deadlock_goal_commitment
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
                fire_progress(
                    SolverPhase::Improving,
                    &budget,
                    &counters,
                    Some(pushes.len() as u32),
                    &mut logger,
                    on_progress,
                );
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
        + counters.deadlock_goal_commitment
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
    logger: &mut PhaseLogger,
) -> SearchOutcome {
    if is_small_puzzle(cb) {
        run_small_puzzle_search(
            cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, mode, logger,
        )
    } else {
        run_large_puzzle_search(
            cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, mode, logger,
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
    logger: &mut PhaseLogger,
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
                cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, logger,
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
    logger: &mut PhaseLogger,
) -> SearchOutcome {
    let beam_result = try_beam(
        cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, logger,
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

    if !budget.exhausted() && cb.initial_box_cells.len() < 12 {
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

/// Shared state for the multi-phase endgame search cascade.
struct EndgameContext<'a> {
    cb: &'a CompiledBoard,
    initial: &'a DenseState,
    zk: &'a ZobristKeys,
    heuristic: &'a mut AssignmentHeuristic,
    deadlocks: &'a DeadlockChecker,
    macros: &'a MacroEngine,
    plan: Option<&'a StructuralPlan>,
    budget: &'a mut Budget,
    counters: &'a mut SearchCounters,
    logger: &'a mut PhaseLogger,
    time_limit: f64,
    box_count: u32,
    checkpoints: Vec<(DenseState, Vec<(usize, Direction)>)>,
    high_pack_checkpoints: Vec<(DenseState, Vec<(usize, Direction)>)>,
}

impl<'a> EndgameContext<'a> {
    fn merge_counters(&mut self, sub: &SearchCounters) {
        self.counters.expanded += sub.expanded;
        self.counters.generated += sub.generated;
        self.counters.heuristic_calls += sub.heuristic_calls;
    }

    /// Phase A: Quick A* from very-high-packing checkpoints.
    fn run_phase_a(&mut self) -> Option<SearchOutcome> {
        let astar_threshold = self.box_count.saturating_sub(2);
        let astar_candidates: Vec<usize> = self.checkpoints.iter()
            .enumerate()
            .filter(|(_, (s, _))| goal_packing_count(self.cb, &s.box_cells) >= astar_threshold)
            .map(|(i, _)| i)
            .take(2)
            .collect();

        if astar_candidates.is_empty() { return None; }

        let astar_per_ms = 3_000u64;
        self.logger.log(LogLevel::Debug, &format!("  endgame quick A*: {} checkpoints (pack >= {}, {}ms each)", astar_candidates.len(), astar_threshold, astar_per_ms));

        for &ci in &astar_candidates {
            if self.budget.exhausted() { break; }
            let checkpoint = self.checkpoints[ci].0.clone();
            let prefix_pushes = self.checkpoints[ci].1.clone();
            let pack = goal_packing_count(self.cb, &checkpoint.box_cells);
            let h = sum_min_distances(self.cb, &checkpoint.box_cells);
            self.heuristic.clear_cache();
            self.logger.log(LogLevel::Debug, &format!("    A* cp={} pack={} h={} prefix={}", ci, pack, h, prefix_pushes.len()));

            let astar_limits = crate::config::SolverLimits {
                max_time_ms: Some(astar_per_ms),
                max_expanded_states: Some(5_000_000),
                max_memory_bytes: Some(400_000_000),
                ..Default::default()
            };
            let mut astar_budget = Budget::new(&astar_limits);
            let mut astar_counters = SearchCounters::default();
            let astar_result = astar_search_quick(
                self.cb, &checkpoint, self.zk, self.heuristic, self.deadlocks,
                &mut astar_budget, &mut astar_counters,
            );
            self.merge_counters(&astar_counters);

            match astar_result {
                AStarResult::Solved { pushes: suffix, .. } => {
                    self.logger.log(LogLevel::Info, &format!("    A* solved! suffix={} pushes (expanded={})", suffix.len(), astar_counters.expanded));
                    let mut full = prefix_pushes;
                    full.extend(suffix);
                    return Some(SearchOutcome::Solved { pushes: full, optimal: false });
                }
                _ => {
                    self.logger.log(LogLevel::Info, &format!("    A* not solved (expanded={}, generated={}, elapsed={:.0}ms)", astar_counters.expanded, astar_counters.generated, astar_budget.elapsed_ms()));
                }
            }
        }
        None
    }

    /// Phase A2: Focused endgame A* with committed on-goal boxes.
    fn run_phase_a2(&mut self) -> Option<SearchOutcome> {
        struct FocusConfig { radius: u32, weight: u32 }
        let focus_configs: &[FocusConfig] = &[
            FocusConfig { radius: 4, weight: 2 },
            FocusConfig { radius: 5, weight: 2 },
            FocusConfig { radius: 6, weight: 2 },
            FocusConfig { radius: 4, weight: 1 },
            FocusConfig { radius: 5, weight: 1 },
            FocusConfig { radius: 8, weight: 2 },
            FocusConfig { radius: 6, weight: 1 },
            FocusConfig { radius: 10, weight: 3 },
        ];
        let focus_per_ms = 3_000u64;
        let focus_max_expanded = 5_000_000u64;
        let focus_max_memory = 300_000_000usize;
        let a2_time_cap = (self.time_limit - self.budget.elapsed_ms()) * 0.10;
        let a2_deadline = self.budget.elapsed_ms() + a2_time_cap;

        let focus_threshold = self.box_count.saturating_sub(3);
        let focus_candidates: Vec<usize> = self.checkpoints.iter()
            .enumerate()
            .filter(|(_, (s, _))| goal_packing_count(self.cb, &s.box_cells) >= focus_threshold)
            .map(|(i, _)| i)
            .take(4)
            .collect();

        if focus_candidates.is_empty() { return None; }

        self.logger.log(LogLevel::Debug, &format!("  Phase A2: focused A* ({} cp x {} cfg, {}ms each, cap={:.0}ms)", focus_candidates.len(), focus_configs.len(), focus_per_ms, a2_time_cap));

        for &ci in &focus_candidates {
            if self.budget.exhausted() || self.budget.elapsed_ms() >= a2_deadline { break; }
            let checkpoint = self.checkpoints[ci].0.clone();
            let prefix_pushes = self.checkpoints[ci].1.clone();
            let pack = goal_packing_count(self.cb, &checkpoint.box_cells);
            let h = sum_min_distances(self.cb, &checkpoint.box_cells);

            for fc in focus_configs {
                if self.budget.exhausted() || self.budget.elapsed_ms() >= a2_deadline { break; }

                let focus_limits = crate::config::SolverLimits {
                    max_time_ms: Some(focus_per_ms),
                    max_expanded_states: Some(focus_max_expanded),
                    max_memory_bytes: Some(focus_max_memory),
                    ..Default::default()
                };
                let mut focus_budget = Budget::new(&focus_limits);
                let mut focus_counters = SearchCounters::default();

                self.logger.log(LogLevel::Debug, &format!("    focused A* cp={} pack={} h={} radius={} w={}", ci, pack, h, fc.radius, fc.weight));

                match endgame_focused_search(
                    self.cb, &checkpoint, self.zk, self.deadlocks,
                    &mut focus_budget, &mut focus_counters, fc.radius, fc.weight,
                ) {
                    EndgameResult::Solved { pushes: suffix } => {
                        self.logger.log(LogLevel::Info, &format!("    SOLVED! suffix={} pushes (expanded={}, generated={})", suffix.len(), focus_counters.expanded, focus_counters.generated));
                        self.merge_counters(&focus_counters);
                        let mut full = prefix_pushes;
                        full.extend(suffix);
                        return Some(SearchOutcome::Solved { pushes: full, optimal: false });
                    }
                    EndgameResult::NotSolved => {
                        self.merge_counters(&focus_counters);
                    }
                }
            }
        }
        None
    }

    /// Phase A3: Greedy DFS with randomized restarts.
    fn run_phase_a3(&mut self) -> Option<SearchOutcome> {
        let a3_time_cap = (self.time_limit - self.budget.elapsed_ms()) * 0.25;
        let a3_deadline = self.budget.elapsed_ms() + a3_time_cap;

        let dfs_threshold = self.box_count.saturating_sub(4);
        let dfs_candidates: Vec<usize> = self.checkpoints.iter()
            .enumerate()
            .filter(|(_, (s, _))| goal_packing_count(self.cb, &s.box_cells) >= dfs_threshold)
            .map(|(i, _)| i)
            .take(6)
            .collect();

        if dfs_candidates.is_empty() { return None; }

        struct DfsConfig { radius: u32, h_slack: u32, depth_limit: u32 }
        let base_configs: &[DfsConfig] = &[
            DfsConfig { radius: 4, h_slack: 30, depth_limit: 25 },
            DfsConfig { radius: 5, h_slack: 30, depth_limit: 25 },
            DfsConfig { radius: 6, h_slack: 30, depth_limit: 25 },
            DfsConfig { radius: 100, h_slack: 8, depth_limit: 20 },
            DfsConfig { radius: 100, h_slack: 15, depth_limit: 25 },
        ];
        let noise_seeds: &[u64] = &[0, 7919, 31337];
        let dfs_per_ms = 5_000u64;

        self.logger.log(LogLevel::Debug, &format!("  Phase A3: greedy DFS ({} cp, {} base cfg x {} seeds, {}ms each, cap={:.0}ms)", dfs_candidates.len(), base_configs.len(), noise_seeds.len(), dfs_per_ms, a3_time_cap));

        for &ci in &dfs_candidates {
            if self.budget.exhausted() || self.budget.elapsed_ms() >= a3_deadline { break; }
            let checkpoint = self.checkpoints[ci].0.clone();
            let prefix_pushes = self.checkpoints[ci].1.clone();
            let pack = goal_packing_count(self.cb, &checkpoint.box_cells);
            let h = sum_min_distances(self.cb, &checkpoint.box_cells);

            for dc in base_configs {
                for &seed in noise_seeds {
                    if self.budget.exhausted() || self.budget.elapsed_ms() >= a3_deadline {
                        return None;
                    }
                    match endgame_greedy_dfs(
                        self.cb, &checkpoint, self.zk, self.deadlocks,
                        dc.radius, dc.h_slack, dc.depth_limit, dfs_per_ms, seed,
                    ) {
                        EndgameResult::Solved { pushes: suffix } => {
                            self.logger.log(LogLevel::Info, &format!("    DFS SOLVED! cp={} pack={} h={} r={} slack={} seed={} suffix={}", ci, pack, h, dc.radius, dc.h_slack, seed, suffix.len()));
                            let mut full = prefix_pushes;
                            full.extend(suffix);
                            return Some(SearchOutcome::Solved { pushes: full, optimal: false });
                        }
                        EndgameResult::NotSolved => {}
                    }
                }
            }
        }
        None
    }

    /// Phase B1: Wide rescue beams from top checkpoints.
    fn run_phase_b1(&mut self) -> Option<SearchOutcome> {
        let endgame_remaining = self.time_limit - self.budget.elapsed_ms();
        let wide_budget_ms = endgame_remaining * 0.20;

        struct RescueConfig { width: usize, seed: u64, hw: f64, div: f64, depth: u32 }
        let wide_configs = [
            RescueConfig { width: 512,  seed: 0,      hw: 0.5, div: 3.0, depth: 300 },
            RescueConfig { width: 256,  seed: 7919,   hw: 0.3, div: 4.0, depth: 500 },
            RescueConfig { width: 1024, seed: 31337,  hw: 0.8, div: 2.5, depth: 200 },
            RescueConfig { width: 128,  seed: 104729, hw: 0.0, div: 5.0, depth: 500 },
        ];

        let num_checkpoints = self.checkpoints.len().min(1);
        let wide_total = num_checkpoints * wide_configs.len();
        let wide_per_ms = (wide_budget_ms / wide_total.max(1) as f64).max(2000.0);

        self.logger.log(LogLevel::Debug, &format!("  Phase B1: {} wide rescue beams ({:.0}ms each, {:.0}ms budget)", wide_total, wide_per_ms, wide_budget_ms));

        for ci in 0..num_checkpoints {
            let checkpoint = self.checkpoints[ci].0.clone();
            let prefix_pushes = self.checkpoints[ci].1.clone();
            let checkpoint_pack = goal_packing_count(self.cb, &checkpoint.box_cells);
            let checkpoint_h = sum_min_distances(self.cb, &checkpoint.box_cells);

            for (ri, rcfg) in wide_configs.iter().enumerate() {
                if self.budget.exhausted() { break; }
                self.heuristic.clear_cache();

                let attempt_deadline = self.budget.elapsed_ms() + wide_per_ms;
                let cont_config = BeamConfig {
                    beam_width: rcfg.width,
                    max_depth: rcfg.depth,
                    seed: rcfg.seed.wrapping_add(ci as u64 * 1000),
                    heuristic_weight: rcfg.hw,
                    diversity_weight: rcfg.div,
                    deadline_ms: Some(attempt_deadline),
                    ..Default::default()
                };
                self.logger.log(LogLevel::Debug, &format!("    wide cp={} cfg={} w={} hw={:.1} div={:.1} pack={} h={} deadline={:.0}ms", ci, ri, rcfg.width, rcfg.hw, rcfg.div, checkpoint_pack, checkpoint_h, attempt_deadline));

                let result = beam_search(
                    self.cb, &checkpoint, self.zk, self.heuristic, self.deadlocks,
                    self.macros, self.budget, self.counters, &cont_config, self.plan,
                );
                match result {
                    BeamResult::Solved { mut incumbents } => {
                        incumbents.sort_by_key(|inc| inc.push_count);
                        let best = incumbents.into_iter().next().unwrap();
                        let mut full = prefix_pushes;
                        full.extend(best.pushes);
                        return Some(SearchOutcome::Solved { pushes: full, optimal: false });
                    }
                    BeamResult::Checkpoints { states } => {
                        let threshold = self.box_count.saturating_sub(2);
                        for (state, pushes) in states {
                            if goal_packing_count(self.cb, &state.box_cells) >= threshold {
                                let mut full_prefix = prefix_pushes.clone();
                                full_prefix.extend(pushes);
                                self.high_pack_checkpoints.push((state, full_prefix));
                            }
                        }
                    }
                    _ => continue,
                }
            }
            if self.budget.exhausted() { break; }
        }
        None
    }

    /// Phase B2: Narrow multi-restart from best checkpoint.
    fn run_phase_b2(&mut self) -> Option<SearchOutcome> {
        if self.budget.exhausted() { return None; }

        let endgame_remaining = self.time_limit - self.budget.elapsed_ms();
        let narrow_budget_ms = endgame_remaining * 0.15;
        let narrow_remaining = (self.time_limit - self.budget.elapsed_ms()).min(narrow_budget_ms);

        let narrow_seeds: &[u64] = &[
            0, 7919, 31337, 104729, 65537, 999983, 314159, 271828,
            1000003, 2000003, 3000017, 4000037, 5000003, 6000011,
            7000003, 8000009, 9000011, 1234567, 7654321, 2718281,
            3141592, 1618033, 4669201, 6931471, 1414213, 1732050,
            2236067, 2645751, 3162277, 3316624, 3605551, 3872983,
        ];
        let narrow_per_ms = 2000.0f64;
        let max_narrow = ((narrow_remaining / narrow_per_ms) as usize).min(narrow_seeds.len());

        let best_checkpoint = self.checkpoints[0].0.clone();
        let best_prefix = self.checkpoints[0].1.clone();
        let best_pack = goal_packing_count(self.cb, &best_checkpoint.box_cells);
        let best_h = sum_min_distances(self.cb, &best_checkpoint.box_cells);

        self.logger.log(LogLevel::Debug, &format!("  Phase B2: {} narrow restarts from best checkpoint (pack={}, h={}, {:.0}ms each)", max_narrow, best_pack, best_h, narrow_per_ms));

        let narrow_widths = [32, 64, 16, 128, 48, 24];
        let narrow_hws = [0.5, 1.0, 0.0, 1.5, 0.3, 2.0];

        for (ni, &seed) in narrow_seeds.iter().take(max_narrow).enumerate() {
            if self.budget.exhausted() { break; }
            self.heuristic.clear_cache();

            let w = narrow_widths[ni % narrow_widths.len()];
            let hw = narrow_hws[ni % narrow_hws.len()];
            let attempt_deadline = self.budget.elapsed_ms() + narrow_per_ms;
            let cont_config = BeamConfig {
                beam_width: w,
                max_depth: 300,
                seed,
                heuristic_weight: hw,
                diversity_weight: 3.0,
                deadline_ms: Some(attempt_deadline),
                ..Default::default()
            };
            self.logger.log(LogLevel::Debug, &format!("    narrow {} w={} hw={:.1} seed={} deadline={:.0}ms", ni, w, hw, seed, attempt_deadline));

            let result = beam_search(
                self.cb, &best_checkpoint, self.zk, self.heuristic, self.deadlocks,
                self.macros, self.budget, self.counters, &cont_config, self.plan,
            );
            match result {
                BeamResult::Solved { mut incumbents } => {
                    incumbents.sort_by_key(|inc| inc.push_count);
                    let best = incumbents.into_iter().next().unwrap();
                    let mut full = best_prefix;
                    full.extend(best.pushes);
                    return Some(SearchOutcome::Solved { pushes: full, optimal: false });
                }
                BeamResult::Checkpoints { states } => {
                    let threshold = self.box_count.saturating_sub(2);
                    for (state, pushes) in states {
                        if goal_packing_count(self.cb, &state.box_cells) >= threshold {
                            let mut full_prefix = best_prefix.clone();
                            full_prefix.extend(pushes);
                            self.high_pack_checkpoints.push((state, full_prefix));
                        }
                    }
                }
                _ => continue,
            }
        }
        None
    }

    /// Phase C: Many small focused beams from high-pack checkpoints.
    fn run_phase_c(&mut self) -> Option<SearchOutcome> {
        if self.budget.exhausted() || self.high_pack_checkpoints.is_empty() {
            return None;
        }

        self.high_pack_checkpoints.sort_by_key(|(s, _)| {
            let pack = goal_packing_count(self.cb, &s.box_cells);
            let h = sum_min_distances(self.cb, &s.box_cells);
            (std::cmp::Reverse(pack), h)
        });
        self.high_pack_checkpoints.dedup_by(|(a, _), (b, _)| a.box_cells == b.box_cells);
        self.high_pack_checkpoints.truncate(4);

        let focused_remaining = self.time_limit - self.budget.elapsed_ms();
        let focus_per_ms = 500.0f64;
        let max_attempts = ((focused_remaining / focus_per_ms) as usize).min(200);

        let best_focus_pack = self.high_pack_checkpoints.first()
            .map(|(s, _)| goal_packing_count(self.cb, &s.box_cells))
            .unwrap_or(0);

        self.logger.log(LogLevel::Debug, &format!("  Phase C: {} checkpoints, up to {} small-beam attempts ({:.0}ms each, best_pack={})", self.high_pack_checkpoints.len(), max_attempts, focus_per_ms, best_focus_pack));

        let focus_widths = [4, 8, 2, 16, 6, 3, 12, 24];
        let focus_divs = [1.0, 2.0, 0.5, 3.0, 1.5, 0.3, 4.0, 0.0];

        for attempt in 0..max_attempts {
            if self.budget.exhausted() { break; }

            let ci = attempt % self.high_pack_checkpoints.len();
            let (checkpoint, prefix) = &self.high_pack_checkpoints[ci];
            let seed = (attempt as u64).wrapping_mul(2654435761);
            let w = focus_widths[attempt % focus_widths.len()];
            let div = focus_divs[attempt % focus_divs.len()];
            let attempt_deadline = self.budget.elapsed_ms() + focus_per_ms;

            let cont_config = BeamConfig {
                beam_width: w,
                max_depth: 200,
                seed,
                heuristic_weight: 0.3,
                diversity_weight: div,
                endgame_focus_radius: Some(4),
                deadline_ms: Some(attempt_deadline),
                ..Default::default()
            };
            if attempt < 10 || attempt % 20 == 0 {
                let pack = goal_packing_count(self.cb, &checkpoint.box_cells);
                let h = sum_min_distances(self.cb, &checkpoint.box_cells);
                self.logger.log(LogLevel::Debug, &format!("    focus #{} cp={} w={} div={:.1} seed={} pack={} h={} deadline={:.0}ms", attempt, ci, w, div, seed, pack, h, attempt_deadline));
            }
            if let BeamResult::Solved { mut incumbents } = beam_search(
                self.cb, checkpoint, self.zk, self.heuristic, self.deadlocks,
                self.macros, self.budget, self.counters, &cont_config, self.plan,
            ) {
                incumbents.sort_by_key(|inc| inc.push_count);
                let best = incumbents.into_iter().next().unwrap();
                let mut full = prefix.clone();
                full.extend(best.pushes);
                self.logger.log(LogLevel::Info, &format!("    SOLVED at attempt #{} (w={}, seed={})", attempt, w, seed));
                return Some(SearchOutcome::Solved { pushes: full, optimal: false });
            }
        }
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn try_beam_large(
    cb: &CompiledBoard,
    initial: &DenseState,
    zk: &ZobristKeys,
    heuristic: &mut AssignmentHeuristic,
    deadlocks: &DeadlockChecker,
    macros: &MacroEngine,
    plan: Option<&StructuralPlan>,
    budget: &mut Budget,
    counters: &mut SearchCounters,
    logger: &mut PhaseLogger,
) -> Option<SearchOutcome> {
    let time_limit = budget.time_limit_ms().unwrap_or(120_000) as f64;
    let box_count = cb.initial_box_cells.len() as u32;

    let mut ctx = EndgameContext {
        cb, initial, zk, heuristic, deadlocks, macros, plan, budget, counters, logger,
        time_limit, box_count,
        checkpoints: Vec::new(),
        high_pack_checkpoints: Vec::new(),
    };

    // Main beam search + extra restarts: collect checkpoints or return solution.
    {
        let configs: Vec<BeamConfig> = vec![
            BeamConfig { beam_width: 256, seed: 0, max_depth: 300, ..Default::default() },
            BeamConfig { beam_width: 512, seed: 7919, max_depth: 200, ..Default::default() },
            BeamConfig { beam_width: 256, seed: 31337, max_depth: 300, ..Default::default() },
        ];

        let base_elapsed = ctx.budget.elapsed_ms();
        let remaining = time_limit - base_elapsed;
        let main_phase_budget = remaining * 0.35;
        let per_config_budget = main_phase_budget / configs.len() as f64;

        for (ci, config) in configs.iter().enumerate() {
            if ctx.budget.exhausted() { break; }

            ctx.heuristic.clear_cache();
            let mut cfg = config.clone();
            cfg.deadline_ms = Some(base_elapsed + (ci as f64 + 1.0) * per_config_budget);
            ctx.logger.log(LogLevel::Debug, &format!(
                "  large beam config {} (w={}, s={}, gp={}, tp={}, deadline={:.0}ms)",
                ci, cfg.beam_width, cfg.seed,
                cfg.goal_packing_weight, cfg.typed_packing_weight,
                cfg.deadline_ms.unwrap_or(0.0)));

            let result = beam_search(
                ctx.cb, ctx.initial, ctx.zk, ctx.heuristic, ctx.deadlocks,
                ctx.macros, ctx.budget, ctx.counters, &cfg, ctx.plan,
            );
            match result {
                BeamResult::Solved { mut incumbents } => {
                    incumbents.sort_by_key(|inc| inc.push_count);
                    let best = incumbents.into_iter().next().unwrap();
                    return Some(SearchOutcome::Solved { pushes: best.pushes, optimal: false });
                }
                BeamResult::Checkpoints { states } => {
                    ctx.logger.log(LogLevel::Debug, &format!("    config {} produced {} checkpoints", ci, states.len()));
                    ctx.checkpoints.extend(states);
                }
                BeamResult::NoSolution | BeamResult::BudgetExceeded => continue,
            }
        }
    }

    // Extra beam restarts with varied scoring profiles.
    {
        struct ExtraConfig { width: usize, seed: u64, hw: f64, wp: f64, ga: f64, div: f64 }
        let extra_configs: &[ExtraConfig] = &[
            ExtraConfig { width: 256, seed: 104729, hw: 3.0, wp: 2.0, ga: 1.0, div: 1.5 },
            ExtraConfig { width: 128, seed: 65537, hw: 4.0, wp: 0.0, ga: 0.0, div: 2.0 },
            ExtraConfig { width: 256, seed: 999983, hw: 2.0, wp: 3.0, ga: 3.0, div: 1.0 },
            ExtraConfig { width: 64, seed: 314159, hw: 5.0, wp: 0.0, ga: 0.0, div: 1.5 },
            ExtraConfig { width: 256, seed: 271828, hw: 1.5, wp: 5.0, ga: 2.0, div: 2.0 },
            ExtraConfig { width: 128, seed: 1000003, hw: 3.0, wp: 1.0, ga: 1.0, div: 3.0 },
            ExtraConfig { width: 32, seed: 2000003, hw: 2.0, wp: 5.0, ga: 2.0, div: 1.5 },
        ];

        let extra_phase_budget = (time_limit - ctx.budget.elapsed_ms()) * 0.35;
        let extra_per_ms = extra_phase_budget / extra_configs.len() as f64;
        let extra_deadline = ctx.budget.elapsed_ms() + extra_phase_budget;

        ctx.logger.log(LogLevel::Debug, &format!("  extra beams from initial: {} configs, {:.0}ms each, {:.0}ms budget", extra_configs.len(), extra_per_ms, extra_phase_budget));

        for ec in extra_configs {
            if ctx.budget.exhausted() || ctx.budget.elapsed_ms() > extra_deadline { break; }

            ctx.heuristic.clear_cache();
            let attempt_deadline = ctx.budget.elapsed_ms() + extra_per_ms;
            let cfg = BeamConfig {
                beam_width: ec.width,
                max_depth: 600,
                seed: ec.seed,
                heuristic_weight: ec.hw,
                wall_pair_weight: ec.wp,
                goal_access_weight: ec.ga,
                diversity_weight: ec.div,
                deadline_ms: Some(attempt_deadline),
                ..Default::default()
            };

            ctx.logger.log(LogLevel::Debug, &format!("    extra w={} s={} hw={:.1} wp={:.1} ga={:.1} div={:.1} deadline={:.0}ms", ec.width, ec.seed, ec.hw, ec.wp, ec.ga, ec.div, attempt_deadline));

            let result = beam_search(
                ctx.cb, ctx.initial, ctx.zk, ctx.heuristic, ctx.deadlocks,
                ctx.macros, ctx.budget, ctx.counters, &cfg, ctx.plan,
            );
            match result {
                BeamResult::Solved { mut incumbents } => {
                    incumbents.sort_by_key(|inc| inc.push_count);
                    let best = incumbents.into_iter().next().unwrap();
                    return Some(SearchOutcome::Solved { pushes: best.pushes, optimal: false });
                }
                BeamResult::Checkpoints { states } => {
                    ctx.checkpoints.extend(states);
                }
                BeamResult::NoSolution | BeamResult::BudgetExceeded => continue,
            }
        }
    }

    if ctx.checkpoints.is_empty() {
        return None;
    }

    // Sort and deduplicate checkpoints for endgame phases.
    ctx.checkpoints.sort_by_key(|(s, _)| {
        let pack = goal_packing_count(cb, &s.box_cells);
        let h = sum_min_distances(cb, &s.box_cells);
        (std::cmp::Reverse(pack), h)
    });
    ctx.checkpoints.dedup_by(|(a, _), (b, _)| a.box_cells == b.box_cells);
    ctx.checkpoints.truncate(24);

    let best_pack = ctx.checkpoints.first()
        .map(|(s, _)| goal_packing_count(cb, &s.box_cells))
        .unwrap_or(0);
    ctx.logger.log(LogLevel::Debug, &format!("  endgame: {} unique checkpoints, best_pack={}", ctx.checkpoints.len(), best_pack));

    // Run endgame phases sequentially, returning on first solution.
    if let Some(outcome) = ctx.run_phase_a() { return Some(outcome); }
    if let Some(outcome) = ctx.run_phase_a2() { return Some(outcome); }
    if let Some(outcome) = ctx.run_phase_a3() { return Some(outcome); }
    if let Some(outcome) = ctx.run_phase_b1() { return Some(outcome); }
    if let Some(outcome) = ctx.run_phase_b2() { return Some(outcome); }
    if let Some(outcome) = ctx.run_phase_c() { return Some(outcome); }

    None
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
    logger: &mut PhaseLogger,
) -> Option<SearchOutcome> {
    let plan_ref = if plan.is_active() { Some(plan) } else { None };

    let box_count = cb.initial_box_cells.len();

    if box_count >= 12 {
        return try_beam_large(cb, initial, zk, heuristic, deadlocks, macros, plan_ref, budget, counters, logger);
    }

    let profiles: &[(usize, u64, u32)] = if box_count >= 8 {
        &[
            (256, 0, 600),
            (128, 7919, 800),
            (64, 31337, 1200),
            (256, 104729, 600),
        ]
    } else {
        &[
            (128, 0, 500),
            (256, 7919, 500),
            (512, 31337, 500),
        ]
    };

    for &(width, seed, depth) in profiles {
        if budget.exhausted() {
            break;
        }

        heuristic.clear_cache();

        let config = BeamConfig {
            beam_width: width,
            max_depth: depth,
            seed,
            ..Default::default()
        };

        match beam_search(
            cb, initial, zk, heuristic, deadlocks, macros, budget, counters, &config, plan_ref,
        ) {
            BeamResult::Solved { mut incumbents } => {
                incumbents.sort_by_key(|inc| inc.push_count);
                let best = incumbents.into_iter().next().unwrap();
                return Some(SearchOutcome::Solved {
                    pushes: best.pushes,
                    optimal: false,
                });
            }
            BeamResult::Checkpoints { states } => {
                let n_checkpoints = states.len().min(8);
                logger.log(LogLevel::Debug, &format!("  launching endgame from {} checkpoints (of {})", n_checkpoints, states.len()));

                // Phase A: try A* from the best 4 checkpoints (optimal for small remaining distance)
                for (ci, (checkpoint, prefix_pushes)) in states.iter().take(n_checkpoints.min(4)).enumerate() {
                    if budget.exhausted() { break; }
                    heuristic.clear_cache();
                    logger.log(LogLevel::Debug, &format!("    A* from checkpoint {} (prefix={})", ci, prefix_pushes.len()));
                    let astar_result = astar_search(cb, checkpoint, zk, heuristic, deadlocks, budget, counters);
                    match astar_result {
                        AStarResult::Solved { pushes: suffix, .. } => {
                            logger.log(LogLevel::Info, &format!("    A* solved! suffix={} pushes", suffix.len()));
                            let mut full = prefix_pushes.clone();
                            full.extend(suffix);
                            return Some(SearchOutcome::Solved {
                                pushes: full,
                                optimal: false,
                            });
                        }
                        _ => continue,
                    }
                }

                // Phase B: beam continuations with reduced wall_pair_weight
                let cont_seeds: &[u64] = &[99991, 199999, 314159, 577213];
                for (ci, (checkpoint, prefix_pushes)) in states.iter().take(n_checkpoints).enumerate() {
                    for &cs in cont_seeds {
                        if budget.exhausted() { break; }
                        heuristic.clear_cache();
                        let cont_config = BeamConfig {
                            beam_width: 256,
                            max_depth: 200,
                            seed: cs.wrapping_add(ci as u64 * 1000),
                            wall_pair_weight: 1.0,
                            ..Default::default()
                        };
                        match beam_search(
                            cb, checkpoint, zk, heuristic, deadlocks, macros, budget, counters, &cont_config, plan_ref,
                        ) {
                            BeamResult::Solved { mut incumbents } => {
                                incumbents.sort_by_key(|inc| inc.push_count);
                                let best = incumbents.into_iter().next().unwrap();
                                let mut full = prefix_pushes.clone();
                                full.extend(best.pushes);
                                return Some(SearchOutcome::Solved {
                                    pushes: full,
                                    optimal: false,
                                });
                            }
                            _ => continue,
                        }
                    }
                    if budget.exhausted() { break; }
                }
            }
            BeamResult::NoSolution | BeamResult::BudgetExceeded => continue,
        }
    }

    None
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
