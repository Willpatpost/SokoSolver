use serde::{Deserialize, Serialize};
use sokomind_solver::pipeline::SolverPhase;

pub const PROTOCOL_VERSION: u32 = 2;

/// Commands sent from the main thread to the web worker.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerCommand {
    Solve {
        puzzle_json: String,
        options_json: String,
    },
    Cancel,
    Ping,
}

/// Responses sent from the web worker to the main thread.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerResponse {
    Progress {
        phase: SolverPhase,
        elapsed_ms: f64,
        expanded_states: u64,
        generated_states: u64,
        best_pushes: Option<u32>,
        best_moves: Option<u32>,
    },
    Result {
        result_json: String,
    },
    Error {
        message: String,
    },
    Pong {
        version: String,
    },
}

/// Versioned envelope wrapping all worker messages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerEnvelope {
    pub version: u32,
    pub payload: WorkerResponse,
}

impl WorkerEnvelope {
    pub fn new(payload: WorkerResponse) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }

    pub fn progress(
        phase: SolverPhase,
        elapsed_ms: f64,
        expanded_states: u64,
        generated_states: u64,
        best_pushes: Option<u32>,
        best_moves: Option<u32>,
    ) -> Self {
        Self::new(WorkerResponse::Progress {
            phase,
            elapsed_ms,
            expanded_states,
            generated_states,
            best_pushes,
            best_moves,
        })
    }

    pub fn result(result_json: String) -> Self {
        Self::new(WorkerResponse::Result { result_json })
    }

    pub fn error(message: String) -> Self {
        Self::new(WorkerResponse::Error { message })
    }

    pub fn pong() -> Self {
        Self::new(WorkerResponse::Pong {
            version: env!("CARGO_PKG_VERSION").into(),
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!(
                r#"{{"version":{},"payload":{{"type":"Error","message":"serialization failed: {}"}}}}"#,
                PROTOCOL_VERSION,
                e.to_string().replace('"', "\\\"")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_roundtrip() {
        let cmd = WorkerCommand::Solve {
            puzzle_json: r#"{"rows":["OOOOO"]}"#.into(),
            options_json: "{}".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: WorkerCommand = serde_json::from_str(&json).unwrap();
        match parsed {
            WorkerCommand::Solve { puzzle_json, .. } => {
                assert!(puzzle_json.contains("rows"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn envelope_progress_json() {
        let env = WorkerEnvelope::progress(
            SolverPhase::Searching,
            1234.5,
            10000,
            50000,
            Some(15),
            None,
        );
        let json = env.to_json();
        assert!(json.contains("\"version\":2"));
        assert!(json.contains("\"Searching\""));
        assert!(json.contains("10000"));
    }

    #[test]
    fn envelope_pong() {
        let env = WorkerEnvelope::pong();
        let json = env.to_json();
        assert!(json.contains("Pong"));
        assert!(json.contains("version"));
    }

    #[test]
    fn cancel_command_parses() {
        let json = r#"{"type":"Cancel"}"#;
        let cmd: WorkerCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, WorkerCommand::Cancel));
    }

    #[test]
    fn envelope_error() {
        let env = WorkerEnvelope::error("something broke".into());
        let json = env.to_json();
        assert!(json.contains("something broke"));
    }
}
