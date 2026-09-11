use serde::{Deserialize, Serialize};
use sokomind_core::game::GameSnapshot;
use sokomind_core::Direction;

use crate::config::SolverRequest;
use crate::log::PhaseLogger;

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
    pub phase_reports: Vec<crate::log::PhaseReport>,
}

/// Top-level entry point. Solves the given puzzle request.
pub fn solve(request: &SolverRequest) -> SolverResult {
    let mut logger = PhaseLogger::new(request.options.log_level);
    logger.start_phase(SolverPhase::Preparing);

    // TODO: Phase 0 — Compile board, build tables, analyze topology
    // TODO: Phase 1 — Structural plan (if large puzzle)
    // TODO: Phase 2 — Beam discovery
    // TODO: Phase 3 — Fallback search
    // TODO: Phase 4 — Harvest + improve
    // TODO: Phase 5 — Proof (quality/optimal mode)
    // TODO: Phase 6 — Verification

    logger.end_phase();

    SolverResult {
        status: SolverStatus::Unsolved {
            reason: "solver not yet implemented".into(),
        },
        solution: None,
        metrics: SolverMetrics {
            elapsed_ms: 0.0,
            expanded_states: 0,
            generated_states: 0,
            peak_frontier: 0,
            peak_memory_bytes: 0,
            deadlock_prunes: 0,
        },
        proof: None,
        phase_reports: logger.into_reports(),
    }
}
