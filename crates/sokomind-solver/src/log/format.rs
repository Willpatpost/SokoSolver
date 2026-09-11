use crate::pipeline::{SolverResult, SolverStatus};

use super::PhaseReport;

/// Format a human-readable solve summary for display or logging.
pub fn format_solve_summary(result: &SolverResult) -> String {
    let mut lines: Vec<String> = Vec::new();

    let status = match &result.status {
        SolverStatus::Solved => "SOLVED",
        SolverStatus::Unsolved { .. } => "UNSOLVED",
        SolverStatus::Cancelled => "CANCELLED",
    };

    lines.push(format!(
        "{} in {:.1}ms (expanded={}, generated={})",
        status,
        result.metrics.elapsed_ms,
        result.metrics.expanded_states,
        result.metrics.generated_states,
    ));

    if let Some(ref sol) = result.solution {
        lines.push(format!(
            "  solution: {} moves, {} pushes",
            sol.moves, sol.pushes,
        ));
    }

    if let Some(ref proof) = result.proof {
        lines.push(format!(
            "  proof: {:?} (lb={}, ub={}, gap={})",
            proof.kind, proof.lower_bound, proof.upper_bound, proof.gap,
        ));
    }

    if let SolverStatus::Unsolved { reason } = &result.status {
        lines.push(format!("  reason: {}", reason));
    }

    if result.metrics.deadlock_prunes > 0 {
        lines.push(format!(
            "  deadlock prunes: {}",
            result.metrics.deadlock_prunes,
        ));
    }

    lines.join("\n")
}

/// Format a single PhaseReport as a compact summary line.
pub fn format_phase_report(report: &PhaseReport) -> String {
    let mut parts = vec![format!("{:?}: {:.1}ms", report.phase, report.elapsed_ms)];

    let mut counters: Vec<(&String, &f64)> = report.counters.iter().collect();
    counters.sort_by_key(|(k, _)| k.as_str());

    for (key, val) in counters {
        if *val == 0.0 {
            continue;
        }
        if val.fract() == 0.0 {
            parts.push(format!("{}={}", key, *val as u64));
        } else {
            parts.push(format!("{}={:.2}", key, val));
        }
    }

    parts.join(", ")
}

/// Format all phase reports as a multi-line summary.
pub fn format_phase_reports(reports: &[PhaseReport]) -> String {
    reports
        .iter()
        .map(format_phase_report)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format telemetry counters as a compact key=value string.
pub fn format_telemetry(telemetry: &crate::log::TelemetryCounters) -> String {
    let map = telemetry.to_counter_map();
    let mut entries: Vec<(&String, &f64)> = map.iter().filter(|(_, v)| **v != 0.0).collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    entries
        .into_iter()
        .map(|(k, v)| {
            if v.fract() == 0.0 {
                format!("{}={}", k, *v as u64)
            } else {
                format!("{}={:.2}", k, v)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{ProofKind, Solution, SolutionStep, SolverMetrics, SolverProof};
    use sokomind_core::position::Direction;

    fn dummy_result() -> SolverResult {
        SolverResult {
            status: SolverStatus::Solved,
            solution: Some(Solution {
                steps: vec![
                    SolutionStep {
                        direction: Direction::Up,
                        pushed: false,
                    },
                    SolutionStep {
                        direction: Direction::Up,
                        pushed: true,
                    },
                ],
                moves: 2,
                pushes: 1,
                final_snapshot: sokomind_core::game::GameSnapshot {
                    robot: sokomind_core::position::Position { row: 1, col: 1 },
                    boxes: vec![],
                    moves: 2,
                    pushes: 1,
                    solved: true,
                },
            }),
            metrics: SolverMetrics {
                elapsed_ms: 42.5,
                expanded_states: 100,
                generated_states: 500,
                peak_frontier: 50,
                peak_memory_bytes: 0,
                deadlock_prunes: 10,
            },
            proof: Some(SolverProof {
                kind: ProofKind::Optimal,
                lower_bound: 1,
                upper_bound: 1,
                gap: 0,
            }),
            telemetry: Default::default(),
            phase_reports: vec![],
        }
    }

    #[test]
    fn summary_contains_status() {
        let result = dummy_result();
        let summary = format_solve_summary(&result);
        assert!(summary.contains("SOLVED"));
        assert!(summary.contains("42.5ms"));
        assert!(summary.contains("2 moves"));
    }

    #[test]
    fn summary_unsolved() {
        let mut result = dummy_result();
        result.status = SolverStatus::Unsolved {
            reason: "budget exceeded".into(),
        };
        result.solution = None;
        result.proof = None;
        let summary = format_solve_summary(&result);
        assert!(summary.contains("UNSOLVED"));
        assert!(summary.contains("budget exceeded"));
    }

    #[test]
    fn phase_report_formatting() {
        let mut counters = std::collections::HashMap::new();
        counters.insert("expanded".into(), 1000.0);
        counters.insert("generated".into(), 5000.0);
        let report = PhaseReport {
            phase: crate::pipeline::SolverPhase::Searching,
            elapsed_ms: 150.3,
            counters,
            sub_reports: vec![],
        };
        let formatted = format_phase_report(&report);
        assert!(formatted.contains("Searching"));
        assert!(formatted.contains("150.3ms"));
        assert!(formatted.contains("expanded=1000"));
    }

    #[test]
    fn telemetry_formatting() {
        let mut tel = crate::log::TelemetryCounters::default();
        tel.expanded_states = 42;
        tel.generated_states = 100;
        let formatted = format_telemetry(&tel);
        assert!(formatted.contains("expanded_states=42"));
        assert!(formatted.contains("generated_states=100"));
    }

    #[test]
    fn zero_counters_omitted() {
        let tel = crate::log::TelemetryCounters::default();
        let formatted = format_telemetry(&tel);
        assert!(formatted.is_empty());
    }
}
