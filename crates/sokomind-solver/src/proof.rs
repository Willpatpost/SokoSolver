use crate::pipeline::{ProofKind, SolverProof};

/// Construct a bounded proof (solution found but optimality not proven).
pub fn bounded_proof(lower_bound: u32, upper_bound: u32) -> SolverProof {
    SolverProof {
        kind: ProofKind::Bounded,
        lower_bound,
        upper_bound,
        gap: upper_bound.saturating_sub(lower_bound),
    }
}

/// Construct an optimal proof (solution proven to be optimal).
pub fn optimal_proof(optimal_cost: u32) -> SolverProof {
    SolverProof {
        kind: ProofKind::Optimal,
        lower_bound: optimal_cost,
        upper_bound: optimal_cost,
        gap: 0,
    }
}

/// Construct an unsolvable proof (no solution exists).
pub fn unsolvable_proof() -> SolverProof {
    SolverProof {
        kind: ProofKind::Unsolvable,
        lower_bound: u32::MAX,
        upper_bound: u32::MAX,
        gap: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_gap() {
        let p = bounded_proof(10, 15);
        assert_eq!(p.kind, ProofKind::Bounded);
        assert_eq!(p.gap, 5);
    }

    #[test]
    fn optimal_zero_gap() {
        let p = optimal_proof(42);
        assert_eq!(p.kind, ProofKind::Optimal);
        assert_eq!(p.gap, 0);
        assert_eq!(p.lower_bound, 42);
    }

    #[test]
    fn unsolvable() {
        let p = unsolvable_proof();
        assert_eq!(p.kind, ProofKind::Unsolvable);
    }
}
