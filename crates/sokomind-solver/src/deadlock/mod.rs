pub mod freeze;
pub mod goal_commitment;
pub mod pattern;
pub mod pi_corral;
pub mod static_dead;
pub mod tables;
pub mod two_by_two;

use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::topology::BoardTopology;

use self::tables::DeadlockTable;

/// Unified deadlock checker that runs the detection pipeline in order
/// of increasing cost: static → 2x2 → freeze → pattern → goal_commitment → pi_corral.
/// The table detector runs early (after freeze) when available.
pub struct DeadlockChecker {
    topo: BoardTopology,
    table: DeadlockTable,
}

impl DeadlockChecker {
    pub fn new(cb: &CompiledBoard) -> Self {
        let topo = BoardTopology::analyze(cb);
        let table = DeadlockTable::build(cb, &topo);
        Self { topo, table }
    }

    pub fn from_topology(topo: BoardTopology, table: DeadlockTable) -> Self {
        Self { topo, table }
    }

    /// Run the full deadlock pipeline. Returns true if deadlocked.
    /// Detectors run cheapest-first; early exit on first detection.
    pub fn is_deadlocked(
        &self,
        cb: &CompiledBoard,
        keeper_pos: u16,
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

        if self.table.is_deadlocked(box_cells) {
            counters.deadlock_table += 1;
            return true;
        }

        if pattern::is_pattern_deadlock(cb, box_cells) {
            counters.deadlock_pattern += 1;
            return true;
        }

        if goal_commitment::is_goal_commitment_deadlock(cb, box_cells) {
            counters.deadlock_goal_commitment += 1;
            return true;
        }

        if pi_corral::is_pi_corral_deadlock(cb, keeper_pos, box_cells) {
            counters.deadlock_pi_corral += 1;
            return true;
        }

        false
    }

    /// Quick check without counter tracking or keeper-dependent detectors.
    pub fn is_deadlocked_quick(&self, cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> bool {
        static_dead::is_static_deadlock(&self.topo, box_cells)
            || two_by_two::is_two_by_two_deadlock(cb, box_cells)
            || freeze::is_freeze_deadlock(cb, box_cells)
            || self.table.is_deadlocked(box_cells)
            || pattern::is_pattern_deadlock(cb, box_cells)
            || goal_commitment::is_goal_commitment_deadlock(cb, box_cells)
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
        assert!(checker.is_deadlocked(&cb, cb.robot_cell, &[(corner, 0)], &mut counters));
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
        assert!(!checker.is_deadlocked(&cb, cb.robot_cell, &[(box_cell, 0)], &mut counters));
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

        let corner = cb.pos_to_cell(Position::new(3, 1));
        assert!(checker.is_deadlocked(&cb, cb.robot_cell, &[(corner, 0)], &mut counters));
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

    #[test]
    fn pipeline_detects_goal_commitment() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let checker = DeadlockChecker::new(&cb);
        let mut counters = SearchCounters::default();

        // Two boxes but only one goal → goal commitment detects this.
        let b1 = cb.pos_to_cell(Position::new(2, 2));
        let b2 = cb.pos_to_cell(Position::new(2, 1));
        let mut boxes = vec![(b1, 0u8), (b2, 0u8)];
        boxes.sort();
        assert!(checker.is_deadlocked(&cb, cb.robot_cell, &boxes, &mut counters));
    }
}
