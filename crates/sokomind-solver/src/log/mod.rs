mod collector;
pub mod format;
mod phase_logger;
mod telemetry;

pub use collector::LogCollector;
pub use phase_logger::{PhaseLogger, PhaseReport};
pub use telemetry::TelemetryCounters;

use serde::{Deserialize, Serialize};

use crate::config::LogLevel;
use crate::pipeline::SolverPhase;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp_ms: f64,
    pub level: LogLevel,
    pub phase: SolverPhase,
    pub span: Option<String>,
    pub message: String,
    pub counters: Option<std::collections::HashMap<String, f64>>,
}
