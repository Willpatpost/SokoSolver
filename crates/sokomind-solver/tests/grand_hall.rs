use sokomind_core::board::parse_board;
use sokomind_core::game::create_snapshot;
use sokomind_solver::config::{LogLevel, SolverLimits, SolverMode, SolverOptions, SolverRequest};
use sokomind_solver::pipeline::{solve, SolverStatus};

#[test]
#[ignore]
fn grand_hall_solves() {
    let rows: Vec<&str> = vec![
        "OOOOOOOOOOOOOOO",
        "OaSS   S   SSbO",
        "OSCS  OOO  SDSO",
        "OX X  OOO  X XO",
        "O     OOO     O",
        "OOOO   X   OOOO",
        "O      O      O",
        "O G hOOOOOH g O",
        "O      O      O",
        "OOO         OOO",
        "OOO   X X   OOO",
        "OOOOOOOROOOOOOO",
        "O B X X X X A O",
        "O Sc       dS O",
        "OOOOOOOOOOOOOOO",
    ];
    let board = parse_board(&rows).unwrap();
    let snapshot = create_snapshot(&board);

    let request = SolverRequest {
        board,
        snapshot,
        limits: SolverLimits {
            max_time_ms: Some(120_000),
            max_expanded_states: Some(2_000_000),
            max_generated_states: Some(50_000_000),
            max_memory_bytes: Some(1024 * 1024 * 1024),
        },
        options: SolverOptions {
            mode: SolverMode::Fast,
            deterministic: false,
            log_level: LogLevel::Info,
            seed: None,
        },
    };

    let result = solve(&request);

    match &result.status {
        SolverStatus::Solved => {
            let solution = result.solution.as_ref().unwrap();
            eprintln!(
                "Grand Hall SOLVED: {} moves, {} pushes in {:.1}ms",
                solution.moves, solution.pushes, result.metrics.elapsed_ms,
            );
            eprintln!(
                "  expanded={}, generated={}, deadlock_prunes={}",
                result.metrics.expanded_states,
                result.metrics.generated_states,
                result.metrics.deadlock_prunes,
            );
            for report in &result.phase_reports {
                eprintln!("  [{:?}] {:.0}ms", report.phase, report.elapsed_ms);
            }
        }
        SolverStatus::Unsolved { reason } => {
            eprintln!(
                "Grand Hall UNSOLVED: {} (elapsed={:.1}ms, expanded={}, generated={})",
                reason,
                result.metrics.elapsed_ms,
                result.metrics.expanded_states,
                result.metrics.generated_states,
            );
        }
        SolverStatus::Cancelled => {
            eprintln!("Grand Hall CANCELLED");
        }
    }

    assert!(
        matches!(result.status, SolverStatus::Solved),
        "Grand Hall should be solved"
    );
}
