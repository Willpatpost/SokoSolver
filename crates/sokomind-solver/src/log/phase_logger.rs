use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::collector::LogCollector;
use super::LogEntry;
use crate::config::LogLevel;
use crate::pipeline::SolverPhase;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaseReport {
    pub phase: SolverPhase,
    pub elapsed_ms: f64,
    pub counters: HashMap<String, f64>,
    pub sub_reports: Vec<PhaseReport>,
}

pub struct PhaseLogger {
    level: LogLevel,
    collector: LogCollector,
    current_phase: Option<SolverPhase>,
    phase_start_ms: f64,
    start_time: std::time::Instant,
    reports: Vec<PhaseReport>,
    current_counters: HashMap<String, f64>,
}

impl PhaseLogger {
    pub fn new(level: LogLevel) -> Self {
        Self {
            level,
            collector: LogCollector::new(10_000),
            current_phase: None,
            phase_start_ms: 0.0,
            start_time: std::time::Instant::now(),
            reports: Vec::new(),
            current_counters: HashMap::new(),
        }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64() * 1000.0
    }

    pub fn start_phase(&mut self, phase: SolverPhase) {
        if self.current_phase.is_some() {
            self.end_phase();
        }
        self.current_phase = Some(phase);
        self.phase_start_ms = self.elapsed_ms();
        self.current_counters.clear();
        self.log(LogLevel::Info, &format!("{:?} started", phase));
    }

    pub fn end_phase(&mut self) {
        if let Some(phase) = self.current_phase.take() {
            let elapsed = self.elapsed_ms() - self.phase_start_ms;
            self.log(
                LogLevel::Info,
                &format!("{:?} completed in {:.1}ms", phase, elapsed),
            );
            self.reports.push(PhaseReport {
                phase,
                elapsed_ms: elapsed,
                counters: std::mem::take(&mut self.current_counters),
                sub_reports: Vec::new(),
            });
        }
    }

    pub fn log(&mut self, level: LogLevel, message: &str) {
        if level < self.level {
            return;
        }
        self.collector.push(LogEntry {
            timestamp_ms: self.elapsed_ms(),
            level,
            phase: self.current_phase.unwrap_or(SolverPhase::Preparing),
            span: None,
            message: message.into(),
            counters: None,
        });
    }

    pub fn log_with_counters(
        &mut self,
        level: LogLevel,
        message: &str,
        counters: HashMap<String, f64>,
    ) {
        if level < self.level {
            return;
        }
        self.collector.push(LogEntry {
            timestamp_ms: self.elapsed_ms(),
            level,
            phase: self.current_phase.unwrap_or(SolverPhase::Preparing),
            span: None,
            message: message.into(),
            counters: Some(counters),
        });
    }

    pub fn counter(&mut self, name: &str, value: f64) {
        *self.current_counters.entry(name.into()).or_insert(0.0) += value;
    }

    pub fn set_counter(&mut self, name: &str, value: f64) {
        self.current_counters.insert(name.into(), value);
    }

    pub fn drain_entries(&mut self) -> Vec<LogEntry> {
        self.collector.drain()
    }

    pub fn into_reports(mut self) -> Vec<PhaseReport> {
        self.end_phase();
        self.reports
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_lifecycle() {
        let mut logger = PhaseLogger::new(LogLevel::Info);
        logger.start_phase(SolverPhase::Searching);
        logger.counter("expanded", 100.0);
        logger.counter("expanded", 50.0);
        logger.set_counter("peak_memory", 1024.0);
        logger.end_phase();

        assert_eq!(logger.reports.len(), 1);
        let report = &logger.reports[0];
        assert_eq!(report.phase, SolverPhase::Searching);
        assert_eq!(report.counters["expanded"], 150.0);
        assert_eq!(report.counters["peak_memory"], 1024.0);
    }

    #[test]
    fn log_level_filtering() {
        let mut logger = PhaseLogger::new(LogLevel::Warn);
        logger.start_phase(SolverPhase::Preparing);
        logger.log(LogLevel::Debug, "should be filtered");
        logger.log(LogLevel::Warn, "should appear");
        // Start/end phase log at Info, which is below Warn, so filtered
        let entries = logger.drain_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "should appear");
    }
}
