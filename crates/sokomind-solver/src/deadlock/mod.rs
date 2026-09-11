pub mod freeze;
pub mod static_dead;
pub mod two_by_two;

use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::topology::BoardTopology;

/// Unified deadlock checker that runs the detection pipeline in order
/// of increasing cost: static → 2x2 → freeze.
pub struct DeadlockChecker {
    topo: BoardTopology,
}

impl DeadlockChecker {
    pub fn new(cb: &CompiledBoard) -> Self {
        let topo = BoardTopology::analyze(cb);
        Self { topo }
    }

    pub fn from_topology(topo: BoardTopology) -> Self {
        Self { topo }
    }

    /// Run the full deadlock pipeline. Returns true if deadlocked.
    /// Updates counters for each detector that fires.
    pub fn is_deadlocked(
        &self,
        cb: &CompiledBoard,
        box_cells: &[(u16, u8)],
        counters: &mut SearchCounters,
    ) -> bool {
        if static_dead::is_static_deadlock(&self.topo, box_cells) {
            counters.deadlock_static += 1;
            return true;
        }

        if two_by_two::is_two_by_two_deadlock(cb, box_cells) {
            counters.deadlock_two_by_two += 1;
            return true;
        }

        if freeze::is_freeze_deadlock(cb, box_cells) {
            counters.deadlock_freeze += 1;
            return true;
        }

        false
    }

    /// Quick check without counter tracking.
    pub fn is_deadlocked_quick(&self, cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> bool {
        static_dead::is_static_deadlock(&self.topo, box_cells)
            || two_by_two::is_two_by_two_deadlock(cb, box_cells)
            || freeze::is_freeze_deadlock(cb, box_cells)
    }

    pub fn topology(&self) -> &BoardTopology {
        &self.topo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn pipeline_detects_static() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let checker = DeadlockChecker::new(&cb);
        let mut counters = SearchCounters::default();

        let corner = cb.pos_to_cell(Position::new(1, 1));
        assert!(checker.is_deadlocked(&cb, &[(corner, 0)], &mut counters));
        assert_eq!(counters.deadlock_static, 1);
        assert_eq!(counters.deadlock_two_by_two, 0);
        assert_eq!(counters.deadlock_freeze, 0);
    }

    #[test]
    fn pipeline_no_deadlock() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let checker = DeadlockChecker::new(&cb);
        let mut counters = SearchCounters::default();

        let box_cell = cb.pos_to_cell(Position::new(2, 2));
        assert!(!checker.is_deadlocked(&cb, &[(box_cell, 0)], &mut counters));
        assert_eq!(counters.deadlock_static, 0);
        assert_eq!(counters.deadlock_two_by_two, 0);
        assert_eq!(counters.deadlock_freeze, 0);
    }

    #[test]
    fn pipeline_detects_freeze() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let checker = DeadlockChecker::new(&cb);
        let mut counters = SearchCounters::default();

        // Box at (3,1): corner, caught by static first
        let corner = cb.pos_to_cell(Position::new(3, 1));
        assert!(checker.is_deadlocked(&cb, &[(corner, 0)], &mut counters));
        // Static catches corners, so freeze won't fire
        assert_eq!(counters.deadlock_static, 1);
    }

    #[test]
    fn quick_check_matches() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let checker = DeadlockChecker::new(&cb);

        let corner = cb.pos_to_cell(Position::new(1, 1));
        assert!(checker.is_deadlocked_quick(&cb, &[(corner, 0)]));

        let open = cb.pos_to_cell(Position::new(2, 2));
        assert!(!checker.is_deadlocked_quick(&cb, &[(open, 0)]));
    }
}
