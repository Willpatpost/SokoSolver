use std::time::Instant;

use crate::config::SolverLimits;

/// Tracks time, state, and memory budgets during search.
pub struct Budget {
    start: Instant,
    limits: SolverLimits,
    expanded: u64,
    generated: u64,
}

impl Budget {
    pub fn new(limits: &SolverLimits) -> Self {
        Self {
            start: Instant::now(),
            limits: limits.clone(),
            expanded: 0,
            generated: 0,
        }
    }

    pub fn tick_expanded(&mut self) {
        self.expanded += 1;
    }

    pub fn tick_generated(&mut self, count: u64) {
        self.generated += count;
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    pub fn exhausted(&self) -> bool {
        if let Some(max_ms) = self.limits.max_time_ms {
            if self.elapsed_ms() >= max_ms as f64 {
                return true;
            }
        }
        if let Some(max_exp) = self.limits.max_expanded_states {
            if self.expanded >= max_exp {
                return true;
            }
        }
        if let Some(max_gen) = self.limits.max_generated_states {
            if self.generated >= max_gen {
                return true;
            }
        }
        false
    }

    pub fn memory_ok(&self, current_bytes: usize) -> bool {
        match self.limits.max_memory_bytes {
            Some(max) => current_bytes < max,
            None => true,
        }
    }

    pub fn expanded(&self) -> u64 {
        self.expanded
    }

    pub fn generated(&self) -> u64 {
        self.generated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SolverLimits;

    #[test]
    fn fresh_budget_not_exhausted() {
        let limits = SolverLimits {
            max_expanded_states: Some(100),
            ..Default::default()
        };
        let budget = Budget::new(&limits);
        assert!(!budget.exhausted());
    }

    #[test]
    fn exhausted_by_expansion() {
        let limits = SolverLimits {
            max_expanded_states: Some(5),
            max_time_ms: None,
            max_generated_states: None,
            max_memory_bytes: None,
        };
        let mut budget = Budget::new(&limits);
        for _ in 0..5 {
            budget.tick_expanded();
        }
        assert!(budget.exhausted());
    }

    #[test]
    fn memory_check() {
        let limits = SolverLimits {
            max_memory_bytes: Some(1024),
            ..Default::default()
        };
        let budget = Budget::new(&limits);
        assert!(budget.memory_ok(512));
        assert!(!budget.memory_ok(2048));
    }
}
