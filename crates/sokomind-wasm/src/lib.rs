mod bridge;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn solver_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

#[wasm_bindgen]
pub fn solve(puzzle_json: &str, options_json: &str) -> String {
    bridge::solve_bridge(puzzle_json, options_json)
}
