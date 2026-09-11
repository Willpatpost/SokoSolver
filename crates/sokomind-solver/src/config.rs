use serde::{Deserialize, Serialize};
use sokomind_core::board::ParsedBoard;
use sokomind_core::game::GameSnapshot;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverRequest {
    pub board: ParsedBoard,
    pub snapshot: GameSnapshot,
    pub limits: SolverLimits,
    pub options: SolverOptions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverLimits {
    pub max_time_ms: Option<u64>,
    pub max_expanded_states: Option<u64>,
    pub max_generated_states: Option<u64>,
    pub max_memory_bytes: Option<usize>,
}

impl Default for SolverLimits {
    fn default() -> Self {
        Self {
            max_time_ms: Some(180_000),
            max_expanded_states: Some(500_000),
            max_generated_states: Some(5_000_000),
            max_memory_bytes: Some(768 * 1024 * 1024),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverOptions {
    pub mode: SolverMode,
    pub deterministic: bool,
    pub log_level: LogLevel,
    pub seed: Option<u64>,
}

impl Default for SolverOptions {
    fn default() -> Self {
        Self {
            mode: SolverMode::Fast,
            deterministic: false,
            log_level: LogLevel::Info,
            seed: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolverMode {
    Fast,
    Quality,
    Optimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}
