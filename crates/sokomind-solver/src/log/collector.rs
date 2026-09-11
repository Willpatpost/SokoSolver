use std::collections::VecDeque;

use super::LogEntry;

/// Ring-buffer log collector with configurable capacity.
pub struct LogCollector {
    entries: VecDeque<LogEntry>,
    capacity: usize,
    total_dropped: u64,
}

impl LogCollector {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
            total_dropped: 0,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
            self.total_dropped += 1;
        }
        self.entries.push_back(entry);
    }

    pub fn drain(&mut self) -> Vec<LogEntry> {
        self.entries.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_dropped(&self) -> u64 {
        self.total_dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LogLevel;
    use crate::pipeline::SolverPhase;

    fn test_entry(msg: &str) -> LogEntry {
        LogEntry {
            timestamp_ms: 0.0,
            level: LogLevel::Info,
            phase: SolverPhase::Searching,
            span: None,
            message: msg.into(),
            counters: None,
        }
    }

    #[test]
    fn ring_buffer_eviction() {
        let mut collector = LogCollector::new(3);
        collector.push(test_entry("a"));
        collector.push(test_entry("b"));
        collector.push(test_entry("c"));
        collector.push(test_entry("d"));
        assert_eq!(collector.len(), 3);
        assert_eq!(collector.total_dropped(), 1);
        let entries = collector.drain();
        assert_eq!(entries[0].message, "b");
        assert_eq!(entries[2].message, "d");
    }

    #[test]
    fn drain_empties() {
        let mut collector = LogCollector::new(10);
        collector.push(test_entry("x"));
        let entries = collector.drain();
        assert_eq!(entries.len(), 1);
        assert!(collector.is_empty());
    }
}
