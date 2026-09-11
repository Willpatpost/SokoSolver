#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::cancellation::CancelToken;
use crate::config::SolverLimits;

/// Tracks time, state, and memory budgets during search.
pub struct Budget {
    start: Instant,
    limits: SolverLimits,
    expanded: u64,
    generated: u64,
    cancel: CancelToken,
}

impl Budget {
    pub fn new(limits: &SolverLimits) -> Self {
        Self {
            start: Instant::now(),
            limits: limits.clone(),
            expanded: 0,
            generated: 0,
            cancel: CancelToken::new(),
        }
    }

    pub fn with_cancel(limits: &SolverLimits, cancel: CancelToken) -> Self {
        Self {
            start: Instant::now(),
            limits: limits.clone(),
            expanded: 0,
            generated: 0,
            cancel,
        }
    }

    pub fn cancel_handle(&self) -> CancelToken {
        self.cancel.clone()
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
        if self.cancel.is_cancelled() {
            return true;
        }
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

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
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

    #[test]
    fn cancel_token_exhausts_budget() {
        let limits = SolverLimits::default();
        let cancel = CancelToken::new();
        let budget = Budget::with_cancel(&limits, cancel.clone());
        assert!(!budget.exhausted());
        cancel.cancel();
        assert!(budget.exhausted());
        assert!(budget.is_cancelled());
    }

    #[test]
    fn cancel_handle_shares_token() {
        let limits = SolverLimits::default();
        let budget = Budget::new(&limits);
        let handle = budget.cancel_handle();
        assert!(!budget.is_cancelled());
        handle.cancel();
        assert!(budget.is_cancelled());
    }
}
