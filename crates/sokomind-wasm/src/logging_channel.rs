use serde::Serialize;
use sokomind_solver::log::LogEntry;

/// Buffered log delivery channel for WASM → JS communication.
///
/// Collects log entries produced during solve and packages them
/// into batches for delivery via the progress callback. Entries
/// are drained on each progress tick to keep memory bounded.
#[derive(Clone, Debug, Serialize)]
pub struct LogBatch {
    pub entries: Vec<LogEntry>,
    pub total_dropped: u64,
}

pub struct LoggingChannel {
    pending: Vec<LogEntry>,
    total_dropped: u64,
    capacity: usize,
}

impl LoggingChannel {
    pub fn new(capacity: usize) -> Self {
        Self {
            pending: Vec::with_capacity(capacity.min(256)),
            total_dropped: 0,
            capacity,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.pending.len() >= self.capacity {
            self.pending.remove(0);
            self.total_dropped += 1;
        }
        self.pending.push(entry);
    }

    pub fn push_batch(&mut self, entries: Vec<LogEntry>) {
        for entry in entries {
            self.push(entry);
        }
    }

    pub fn drain(&mut self) -> Option<LogBatch> {
        if self.pending.is_empty() {
            return None;
        }
        Some(LogBatch {
            entries: std::mem::take(&mut self.pending),
            total_dropped: self.total_dropped,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for LoggingChannel {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_solver::config::LogLevel;
    use sokomind_solver::pipeline::SolverPhase;

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
    fn drain_returns_entries() {
        let mut ch = LoggingChannel::new(100);
        ch.push(test_entry("hello"));
        ch.push(test_entry("world"));
        let batch = ch.drain().unwrap();
        assert_eq!(batch.entries.len(), 2);
        assert_eq!(batch.total_dropped, 0);
        assert!(ch.is_empty());
    }

    #[test]
    fn drain_empty_returns_none() {
        let mut ch = LoggingChannel::new(100);
        assert!(ch.drain().is_none());
    }

    #[test]
    fn capacity_eviction() {
        let mut ch = LoggingChannel::new(3);
        ch.push(test_entry("a"));
        ch.push(test_entry("b"));
        ch.push(test_entry("c"));
        ch.push(test_entry("d"));
        assert_eq!(ch.len(), 3);
        let batch = ch.drain().unwrap();
        assert_eq!(batch.entries[0].message, "b");
        assert_eq!(batch.entries[2].message, "d");
        assert_eq!(batch.total_dropped, 1);
    }

    #[test]
    fn push_batch_works() {
        let mut ch = LoggingChannel::new(100);
        ch.push_batch(vec![test_entry("x"), test_entry("y")]);
        assert_eq!(ch.len(), 2);
    }
}
