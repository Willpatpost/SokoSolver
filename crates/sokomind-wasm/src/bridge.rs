use serde::Deserialize;
use sokomind_core::board::parse_board;
use sokomind_core::game::create_snapshot;
use sokomind_solver::cancellation::CancelToken;
use sokomind_solver::config::{LogLevel, SolverLimits, SolverMode, SolverOptions, SolverRequest};
use sokomind_solver::pipeline::{solve, solve_with_progress, ProgressUpdate};
use wasm_bindgen::prelude::*;

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

fn parse_request(puzzle_json: &str, options_json: &str) -> Result<SolverRequest, String> {
    let puzzle: WasmPuzzleInput =
        serde_json::from_str(puzzle_json).map_err(|e| format!("invalid puzzle JSON: {}", e))?;

    let opts: WasmOptionsInput =
        serde_json::from_str(options_json).map_err(|e| format!("invalid options JSON: {}", e))?;

    let row_refs: Vec<&str> = puzzle.rows.iter().map(|s| s.as_str()).collect();
    let board = parse_board(&row_refs).map_err(|e| format!("invalid board: {}", e))?;

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

    Ok(SolverRequest {
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
    })
}

pub fn solve_bridge(puzzle_json: &str, options_json: &str) -> String {
    let request = match parse_request(puzzle_json, options_json) {
        Ok(r) => r,
        Err(e) => return error_result(&e),
    };

    let result = solve(&request);
    serde_json::to_string(&result)
        .unwrap_or_else(|e| error_result(&format!("serialization failed: {}", e)))
}

/// Solve with periodic progress callbacks to a JS function.
///
/// The `on_progress` function receives a JSON-serialized `ProgressUpdate`
/// and returns a boolean: `true` to continue, `false` to cancel.
pub fn solve_bridge_with_progress(
    puzzle_json: &str,
    options_json: &str,
    on_progress: &js_sys::Function,
) -> String {
    let request = match parse_request(puzzle_json, options_json) {
        Ok(r) => r,
        Err(e) => return error_result(&e),
    };

    let cancel = CancelToken::new();
    let cancel_for_cb = cancel.clone();

    let mut callback = |update: &ProgressUpdate| -> bool {
        let json = match serde_json::to_string(update) {
            Ok(j) => j,
            Err(_) => return true,
        };
        let result = on_progress.call1(&JsValue::NULL, &JsValue::from_str(&json));
        match result {
            Ok(val) => {
                if val.as_bool() == Some(false) {
                    cancel_for_cb.cancel();
                    false
                } else {
                    true
                }
            }
            Err(_) => {
                cancel_for_cb.cancel();
                false
            }
        }
    };

    let result = solve_with_progress(&request, cancel, &mut callback);
    serde_json::to_string(&result)
        .unwrap_or_else(|e| error_result(&format!("serialization failed: {}", e)))
}

fn error_result(message: &str) -> String {
    format!(
        r#"{{"status":{{"Unsolved":{{"reason":"{}"}}}}}}"#,
        message.replace('"', "\\\"")
    )
}
