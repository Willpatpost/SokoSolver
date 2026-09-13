use sokomind_core::board::parse_board;
use sokomind_core::game::create_snapshot;
use sokomind_solver::config::{LogLevel, SolverLimits, SolverMode, SolverOptions, SolverRequest};
use sokomind_solver::pipeline::{solve, SolverStatus};

fn make_request(rows: &[&str], mode: SolverMode) -> SolverRequest {
    let board = parse_board(rows).unwrap();
    let snapshot = create_snapshot(&board);
    SolverRequest {
        board,
        snapshot,
        limits: SolverLimits {
            max_time_ms: Some(30_000),
            max_expanded_states: Some(1_000_000),
            max_generated_states: None,
            max_memory_bytes: Some(512 * 1024 * 1024),
        },
        options: SolverOptions {
            mode,
            deterministic: true,
            log_level: LogLevel::Info,
            seed: Some(42),
        },
    }
}

struct KnownOptimum {
    name: &'static str,
    rows: &'static [&'static str],
    optimal_pushes: u32,
}

const KNOWN_OPTIMA: &[KnownOptimum] = &[
    KnownOptimum {
        name: "trivial-1box",
        rows: &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"],
        optimal_pushes: 2,
    },
    KnownOptimum {
        name: "trivial-1box-short",
        rows: &["OOOOO", "OSXRO", "OOOOO"],
        optimal_pushes: 1,
    },
    KnownOptimum {
        name: "two-box-horizontal",
        rows: &[
            "OOOOOOO", "O S S O", "O     O", "O X X O", "O  R  O", "OOOOOOO",
        ],
        optimal_pushes: 4,
    },
    KnownOptimum {
        name: "typed-2box",
        rows: &[
            "OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO",
        ],
        optimal_pushes: 4,
    },
    KnownOptimum {
        name: "l-push",
        rows: &["OOOOO", "O  SO", "O X O", "O R O", "OOOOO"],
        optimal_pushes: 2,
    },
    // ── Medium puzzles (3-5 boxes) ──
    KnownOptimum {
        name: "3box-straight",
        rows: &[
            "OOOOOOO",
            "OSSS  O",
            "O     O",
            "OXXX  O",
            "O  R  O",
            "O     O",
            "OOOOOOO",
        ],
        optimal_pushes: 6,
    },
    KnownOptimum {
        name: "3box-tall",
        rows: &[
            "OOOOOOO",
            "O SSS O",
            "O     O",
            "O     O",
            "O XXX O",
            "O  R  O",
            "O     O",
            "OOOOOOO",
        ],
        optimal_pushes: 9,
    },
    KnownOptimum {
        name: "4box-separated",
        rows: &[
            "OOOOOOOOO",
            "O S   S O",
            "O       O",
            "O X   X O",
            "O   R   O",
            "O X   X O",
            "O       O",
            "O S   S O",
            "OOOOOOOOO",
        ],
        optimal_pushes: 8,
    },
    KnownOptimum {
        name: "5box-wide",
        rows: &[
            "OOOOOOOOO",
            "OSSSSS  O",
            "O       O",
            "OXXXXXR O",
            "O       O",
            "OOOOOOOOO",
        ],
        optimal_pushes: 10,
    },
];

#[test]
fn known_optima_fast_mode() {
    for fixture in KNOWN_OPTIMA {
        let request = make_request(fixture.rows, SolverMode::Fast);
        let result = solve(&request);

        match &result.status {
            SolverStatus::Solved => {
                let sol = result.solution.as_ref().unwrap();
                assert!(
                    sol.pushes <= fixture.optimal_pushes * 2,
                    "{}: fast mode produced {} pushes, expected at most {} (2x optimal {})",
                    fixture.name,
                    sol.pushes,
                    fixture.optimal_pushes * 2,
                    fixture.optimal_pushes,
                );
                assert!(
                    sol.final_snapshot.solved,
                    "{}: solution did not reach solved state",
                    fixture.name,
                );
            }
            other => panic!("{}: expected Solved, got {:?}", fixture.name, other),
        }
    }
}

#[test]
fn known_optima_optimal_mode() {
    for fixture in KNOWN_OPTIMA {
        let request = make_request(fixture.rows, SolverMode::Optimal);
        let result = solve(&request);

        match &result.status {
            SolverStatus::Solved => {
                let sol = result.solution.as_ref().unwrap();
                assert_eq!(
                    sol.pushes, fixture.optimal_pushes,
                    "{}: optimal mode found {} pushes, expected exactly {}",
                    fixture.name, sol.pushes, fixture.optimal_pushes,
                );
                assert!(
                    sol.final_snapshot.solved,
                    "{}: solution did not reach solved state",
                    fixture.name,
                );

                if let Some(ref proof) = result.proof {
                    assert_eq!(
                        proof.gap, 0,
                        "{}: proof gap should be 0 for optimal solution",
                        fixture.name,
                    );
                }
            }
            other => panic!("{}: expected Solved, got {:?}", fixture.name, other),
        }
    }
}

const KNOWN_UNSOLVABLE: &[(&str, &[&str])] = &[(
    "box-against-wall",
    &["OOOOO", "OX  O", "O  SO", "O  RO", "OOOOO"],
)];

#[test]
fn known_unsolvable_detected() {
    for &(name, rows) in KNOWN_UNSOLVABLE {
        let request = make_request(rows, SolverMode::Fast);
        let result = solve(&request);

        match &result.status {
            SolverStatus::Unsolved { reason } => {
                assert!(
                    reason.contains("exhausted") || reason.contains("budget"),
                    "{}: expected exhausted/budget reason, got: {}",
                    name,
                    reason,
                );
            }
            SolverStatus::Cancelled => {
                panic!("{}: unexpected cancellation", name);
            }
            SolverStatus::Solved => {
                panic!("{}: should not solve an unsolvable puzzle", name);
            }
        }
    }
}

#[test]
fn solve_result_has_telemetry() {
    let request = make_request(
        &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"],
        SolverMode::Fast,
    );
    let result = solve(&request);

    assert!(result.metrics.expanded_states > 0);
    assert!(result.metrics.elapsed_ms > 0.0);
    assert!(!result.phase_reports.is_empty());
}

#[test]
fn solve_with_progress_reports_phases() {
    use sokomind_solver::cancellation::CancelToken;
    use sokomind_solver::pipeline::{solve_with_progress, ProgressUpdate, SolverPhase};

    let request = make_request(
        &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"],
        SolverMode::Fast,
    );

    let mut phases_seen: Vec<SolverPhase> = Vec::new();
    let cancel = CancelToken::new();

    let result = solve_with_progress(&request, cancel, &mut |update: &ProgressUpdate| {
        phases_seen.push(update.phase);
        true
    });

    assert!(matches!(result.status, SolverStatus::Solved));
    assert!(
        phases_seen.contains(&SolverPhase::Preparing),
        "should report Preparing phase"
    );
    assert!(
        phases_seen.contains(&SolverPhase::Searching),
        "should report Searching phase"
    );
}

#[test]
fn cancellation_during_solve() {
    use sokomind_solver::cancellation::CancelToken;
    use sokomind_solver::pipeline::solve_with_progress;

    let request = make_request(
        &[
            "OOOOOOOOO",
            "OSSS    O",
            "O       O",
            "O XXX   O",
            "O   R   O",
            "O       O",
            "OOOOOOOOO",
        ],
        SolverMode::Fast,
    );

    let cancel = CancelToken::new();
    let cancel_clone = cancel.clone();

    let mut call_count = 0;
    let result = solve_with_progress(&request, cancel, &mut |_| {
        call_count += 1;
        if call_count >= 2 {
            cancel_clone.cancel();
            return false;
        }
        true
    });

    assert!(
        matches!(result.status, SolverStatus::Cancelled)
            || matches!(result.status, SolverStatus::Solved),
        "should be Cancelled or Solved (if solved before cancel fired)"
    );
}
