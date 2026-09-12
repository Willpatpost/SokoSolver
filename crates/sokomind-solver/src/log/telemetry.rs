use serde::{Deserialize, Serialize};

/// Structured telemetry counters accumulated during search.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TelemetryCounters {
    pub expanded_states: u64,
    pub generated_states: u64,
    pub duplicate_states: u64,
    pub peak_frontier: u64,
    pub peak_memory_bytes: usize,

    pub deadlock_static_prunes: u64,
    pub deadlock_two_by_two_prunes: u64,
    pub deadlock_freeze_prunes: u64,
    pub deadlock_pattern_prunes: u64,
    pub deadlock_goal_commitment_prunes: u64,
    pub deadlock_pi_corral_prunes: u64,
    pub deadlock_table_prunes: u64,

    pub heuristic_calls: u64,
    pub heuristic_cache_hits: u64,

    pub macro_forced_push: u64,
    pub macro_tunnel: u64,
    pub macro_goal: u64,

    pub transposition_unique: u64,
    pub transposition_duplicate: u64,
}

impl TelemetryCounters {
    pub fn total_deadlock_prunes(&self) -> u64 {
        self.deadlock_static_prunes
            + self.deadlock_two_by_two_prunes
            + self.deadlock_freeze_prunes
            + self.deadlock_pattern_prunes
            + self.deadlock_goal_commitment_prunes
            + self.deadlock_pi_corral_prunes
            + self.deadlock_table_prunes
    }

    pub fn to_counter_map(&self) -> std::collections::HashMap<String, f64> {
        let mut m = std::collections::HashMap::new();
        m.insert("expanded_states".into(), self.expanded_states as f64);
        m.insert("generated_states".into(), self.generated_states as f64);
        m.insert("duplicate_states".into(), self.duplicate_states as f64);
        m.insert("peak_frontier".into(), self.peak_frontier as f64);
        m.insert("peak_memory_bytes".into(), self.peak_memory_bytes as f64);
        m.insert("deadlock.static".into(), self.deadlock_static_prunes as f64);
        m.insert(
            "deadlock.two_by_two".into(),
            self.deadlock_two_by_two_prunes as f64,
        );
        m.insert("deadlock.freeze".into(), self.deadlock_freeze_prunes as f64);
        m.insert(
            "deadlock.pattern".into(),
            self.deadlock_pattern_prunes as f64,
        );
        m.insert(
            "deadlock.goal_commitment".into(),
            self.deadlock_goal_commitment_prunes as f64,
        );
        m.insert(
            "deadlock.pi_corral".into(),
            self.deadlock_pi_corral_prunes as f64,
        );
        m.insert("deadlock.table".into(), self.deadlock_table_prunes as f64);
        m.insert("heuristic.calls".into(), self.heuristic_calls as f64);
        m.insert(
            "heuristic.cache_hits".into(),
            self.heuristic_cache_hits as f64,
        );
        m.insert("macro.forced_push".into(), self.macro_forced_push as f64);
        m.insert("macro.tunnel".into(), self.macro_tunnel as f64);
        m.insert("macro.goal".into(), self.macro_goal as f64);
        m.insert(
            "transposition.unique".into(),
            self.transposition_unique as f64,
        );
        m.insert(
            "transposition.duplicate".into(),
            self.transposition_duplicate as f64,
        );
        m
    }
}
