mod bridge;
pub mod logging_channel;
pub mod worker_protocol;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn solver_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

#[wasm_bindgen]
pub fn solve(puzzle_json: &str, options_json: &str) -> String {
    bridge::solve_bridge(puzzle_json, options_json)
}

/// Solve a puzzle with periodic progress callbacks.
///
/// `on_progress` receives a JSON string with phase, elapsed time,
/// and search counters. Return `true` to continue or `false` to cancel.
#[wasm_bindgen]
pub fn solve_with_progress(
    puzzle_json: &str,
    options_json: &str,
    on_progress: &js_sys::Function,
) -> String {
    bridge::solve_bridge_with_progress(puzzle_json, options_json, on_progress)
}

/// Parse a worker command from JSON.
///
/// Returns JSON-serialized result: either the parsed command
/// or an error envelope. Used by the worker to decode incoming messages.
#[wasm_bindgen]
pub fn parse_worker_command(json: &str) -> String {
    match serde_json::from_str::<worker_protocol::WorkerCommand>(json) {
        Ok(_) => json.to_string(),
        Err(e) => {
            worker_protocol::WorkerEnvelope::error(format!("invalid command: {}", e)).to_json()
        }
    }
}

/// Get the worker protocol version.
#[wasm_bindgen]
pub fn protocol_version() -> u32 {
    worker_protocol::PROTOCOL_VERSION
}
