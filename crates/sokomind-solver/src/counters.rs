use crate::log::TelemetryCounters;

/// Mutable counter accumulator used during a single search pass.
#[derive(Clone, Debug, Default)]
pub struct SearchCounters {
    pub expanded: u64,
    pub generated: u64,
    pub duplicates: u64,
    pub peak_frontier: u64,
    pub deadlock_static: u64,
    pub deadlock_two_by_two: u64,
    pub deadlock_freeze: u64,
    pub deadlock_pattern: u64,
    pub deadlock_goal_commitment: u64,
    pub deadlock_pi_corral: u64,
    pub deadlock_table: u64,
    pub heuristic_calls: u64,
    pub heuristic_cache_hits: u64,
    pub macro_forced_push: u64,
    pub macro_tunnel: u64,
    pub macro_goal: u64,
    pub transposition_unique: u64,
    pub transposition_duplicate: u64,
}

impl SearchCounters {
    pub fn to_telemetry(&self, peak_memory: usize) -> TelemetryCounters {
        TelemetryCounters {
            expanded_states: self.expanded,
            generated_states: self.generated,
            duplicate_states: self.duplicates,
            peak_frontier: self.peak_frontier,
            peak_memory_bytes: peak_memory,
            deadlock_static_prunes: self.deadlock_static,
            deadlock_two_by_two_prunes: self.deadlock_two_by_two,
            deadlock_freeze_prunes: self.deadlock_freeze,
            deadlock_pattern_prunes: self.deadlock_pattern,
            deadlock_goal_commitment_prunes: self.deadlock_goal_commitment,
            deadlock_pi_corral_prunes: self.deadlock_pi_corral,
            deadlock_table_prunes: self.deadlock_table,
            heuristic_calls: self.heuristic_calls,
            heuristic_cache_hits: self.heuristic_cache_hits,
            macro_forced_push: self.macro_forced_push,
            macro_tunnel: self.macro_tunnel,
            macro_goal: self.macro_goal,
            transposition_unique: self.transposition_unique,
            transposition_duplicate: self.transposition_duplicate,
        }
    }

    pub fn update_peak_frontier(&mut self, current: u64) {
        if current > self.peak_frontier {
            self.peak_frontier = current;
        }
    }
}
