use rustc_hash::FxHashMap;

/// Entry stored in the transposition table.
#[derive(Clone, Debug)]
pub struct TranspositionEntry {
    pub pushes: u32,
    pub moves: u32,
}

/// Hash-based transposition table for visited states.
/// Key is a Zobrist hash (u64). Stores best-known cost for each state.
pub struct TranspositionTable {
    map: FxHashMap<u64, TranspositionEntry>,
}

impl TranspositionTable {
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    /// Try to insert a state. Returns true if this is a new state or improves
    /// on the existing entry (fewer pushes, or same pushes with fewer moves).
    pub fn insert(&mut self, hash: u64, pushes: u32, moves: u32) -> bool {
        match self.map.get(&hash) {
            Some(existing) => {
                if pushes < existing.pushes || (pushes == existing.pushes && moves < existing.moves)
                {
                    self.map.insert(hash, TranspositionEntry { pushes, moves });
                    true
                } else {
                    false
                }
            }
            None => {
                self.map.insert(hash, TranspositionEntry { pushes, moves });
                true
            }
        }
    }

    /// Returns true if the table already contains this hash with equal or
    /// better cost, meaning the proposed state is dominated and should be
    /// skipped. Does NOT insert.
    pub fn dominates(&self, hash: u64, pushes: u32, moves: u32) -> bool {
        match self.map.get(&hash) {
            Some(existing) => {
                existing.pushes < pushes || (existing.pushes == pushes && existing.moves <= moves)
            }
            None => false,
        }
    }

    pub fn get(&self, hash: u64) -> Option<&TranspositionEntry> {
        self.map.get(&hash)
    }

    pub fn contains(&self, hash: u64) -> bool {
        self.map.contains_key(&hash)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn estimated_memory_bytes(&self) -> usize {
        self.map.len()
            * (std::mem::size_of::<u64>() + std::mem::size_of::<TranspositionEntry>() + 8)
    }
}

impl Default for TranspositionTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_new_returns_true() {
        let mut tt = TranspositionTable::new();
        assert!(tt.insert(123, 5, 20));
        assert_eq!(tt.len(), 1);
    }

    #[test]
    fn duplicate_returns_false() {
        let mut tt = TranspositionTable::new();
        tt.insert(123, 5, 20);
        assert!(!tt.insert(123, 5, 20));
        assert!(!tt.insert(123, 6, 10));
    }

    #[test]
    fn better_pushes_replaces() {
        let mut tt = TranspositionTable::new();
        tt.insert(123, 5, 20);
        assert!(tt.insert(123, 4, 25));
        let entry = tt.get(123).unwrap();
        assert_eq!(entry.pushes, 4);
        assert_eq!(entry.moves, 25);
    }

    #[test]
    fn same_pushes_fewer_moves_replaces() {
        let mut tt = TranspositionTable::new();
        tt.insert(123, 5, 20);
        assert!(tt.insert(123, 5, 15));
        let entry = tt.get(123).unwrap();
        assert_eq!(entry.moves, 15);
    }

    #[test]
    fn contains_check() {
        let mut tt = TranspositionTable::new();
        assert!(!tt.contains(42));
        tt.insert(42, 1, 1);
        assert!(tt.contains(42));
    }

    #[test]
    fn memory_estimate_nonzero() {
        let mut tt = TranspositionTable::new();
        for i in 0..100 {
            tt.insert(i, 0, 0);
        }
        assert!(tt.estimated_memory_bytes() > 0);
    }
}
