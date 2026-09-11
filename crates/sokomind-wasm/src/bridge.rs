use serde::Deserialize;
use sokomind_core::board::parse_board;
use sokomind_core::game::create_snapshot;
use sokomind_solver::config::{LogLevel, SolverLimits, SolverMode, SolverOptions, SolverRequest};
use sokomind_solver::pipeline::solve;

#[derive(Deserialize)]
struct WasmPuzzleInput {
    rows: Vec<String>,
}

#[derive(Deserialize)]
#[serde(default)]
struct WasmOptionsInput {
    mode: String,
    deterministic: bool,
    log_level: String,
    max_time_ms: Option<u64>,
    max_expanded_states: Option<u64>,
    max_memory_bytes: Option<usize>,
    seed: Option<u64>,
}

impl Default for WasmOptionsInput {
    fn default() -> Self {
        Self {
            mode: "fast".into(),
            deterministic: false,
            log_level: "info".into(),
            max_time_ms: Some(180_000),
            max_expanded_states: Some(500_000),
            max_memory_bytes: Some(768 * 1024 * 1024),
            seed: None,
        }
    }
}

pub fn solve_bridge(puzzle_json: &str, options_json: &str) -> String {
    let puzzle: WasmPuzzleInput = match serde_json::from_str(puzzle_json) {
        Ok(p) => p,
        Err(e) => return error_result(&format!("invalid puzzle JSON: {}", e)),
    };

    let opts: WasmOptionsInput = match serde_json::from_str(options_json) {
        Ok(o) => o,
        Err(e) => return error_result(&format!("invalid options JSON: {}", e)),
    };

    let row_refs: Vec<&str> = puzzle.rows.iter().map(|s| s.as_str()).collect();
    let board = match parse_board(&row_refs) {
        Ok(b) => b,
        Err(e) => return error_result(&format!("invalid board: {}", e)),
    };

    let snapshot = create_snapshot(&board);

    let mode = match opts.mode.as_str() {
        "fast" => SolverMode::Fast,
        "quality" => SolverMode::Quality,
        "optimal" => SolverMode::Optimal,
        _ => SolverMode::Fast,
    };

    let log_level = match opts.log_level.as_str() {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    };

    let request = SolverRequest {
        board,
        snapshot,
        limits: SolverLimits {
            max_time_ms: opts.max_time_ms,
            max_expanded_states: opts.max_expanded_states,
            max_generated_states: None,
            max_memory_bytes: opts.max_memory_bytes,
        },
        options: SolverOptions {
            mode,
            deterministic: opts.deterministic,
            log_level,
            seed: opts.seed,
        },
    };

    let result = solve(&request);
    serde_json::to_string(&result).unwrap_or_else(|e| error_result(&format!("serialization failed: {}", e)))
}

fn error_result(message: &str) -> String {
    format!(
        r#"{{"status":{{"Unsolved":{{"reason":"{}"}}}}}}"#,
        message.replace('"', "\\\"")
    )
}
