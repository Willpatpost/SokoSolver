use sokomind_core::board::parse_board;
use sokomind_core::game::create_snapshot;
use sokomind_solver::config::{LogLevel, SolverLimits, SolverMode, SolverOptions, SolverRequest};
use sokomind_solver::pipeline::solve;

#[test]
fn tutorial_puzzle_solves() {
    let rows = vec!["OOOOO", "O R O", "O A O", "O a O", "OOOOO"];
    let board = parse_board(&rows).expect("parse failed");
    let snapshot = create_snapshot(&board);
    let request = SolverRequest {
        board,
        snapshot,
        limits: SolverLimits {
            max_time_ms: Some(30_000),
            max_expanded_states: Some(500_000),
            max_generated_states: None,
            max_memory_bytes: None,
        },
        options: SolverOptions {
            mode: SolverMode::Fast,
            deterministic: false,
            log_level: LogLevel::Info,
            seed: None,
        },
    };
    let result = solve(&request);
    let json = serde_json::to_string_pretty(&result).unwrap();
    println!("RESULT JSON:\n{}", json);

    match &result.status {
        sokomind_solver::pipeline::SolverStatus::Solved => println!("STATUS: Solved"),
        sokomind_solver::pipeline::SolverStatus::Unsolved { reason } => {
            println!("STATUS: Unsolved - {}", reason)
        }
        sokomind_solver::pipeline::SolverStatus::Cancelled => println!("STATUS: Cancelled"),
    }
    assert!(matches!(
        result.status,
        sokomind_solver::pipeline::SolverStatus::Solved
    ));
    assert!(result.solution.is_some());
}
