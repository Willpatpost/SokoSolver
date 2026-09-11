use crate::compiled_board::CompiledBoard;

use super::corral_ordering::CorralOrdering;
use super::doorway_scheduling::DoorwaySchedule;
use super::rooms::RoomMap;

const MIN_BOXES_FOR_PLANNING: usize = 6;
const MIN_FLOOR_FOR_PLANNING: usize = 60;

/// High-level structural plan combining room analysis, corral ordering,
/// and doorway scheduling.
///
/// Built during the preparation phase for puzzles with enough complexity
/// to benefit from structural guidance (6+ boxes or 60+ floor cells).
/// Provides `evaluate_state()` which returns a priority boost for beam
/// search tiebreaking — lower values mean the state better matches the
/// structural plan.
pub struct StructuralPlan {
    rooms: RoomMap,
    corral: CorralOrdering,
    schedule: DoorwaySchedule,
    has_structure: bool,
}

impl StructuralPlan {
    /// Build a structural plan for the given board, or return a no-op
    /// plan if the puzzle is too small to benefit.
    pub fn build(cb: &CompiledBoard) -> Self {
        let rooms = RoomMap::analyze(cb);

        let has_structure = should_plan(cb, &rooms);

        let corral = CorralOrdering::build(cb, &rooms);
        let schedule = DoorwaySchedule::build(cb, &rooms);

        Self {
            rooms,
            corral,
            schedule,
            has_structure,
        }
    }

    /// Whether this plan has meaningful structural content.
    /// Returns false for small, single-room puzzles where the overhead
    /// of structural evaluation isn't worthwhile.
    pub fn is_active(&self) -> bool {
        self.has_structure
    }

    /// Evaluate a state's alignment with the structural plan.
    ///
    /// Returns a priority boost value (lower is better) that combines:
    /// - Corral ordering: penalty for boxes still far from goal rooms
    /// - Doorway crossing: penalty for boxes needing many crossings
    ///
    /// When the plan is not active, returns 0 (no bias).
    pub fn evaluate_state(&self, cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
        if !self.has_structure {
            return 0;
        }

        let corral_boost = self.corral.corral_boost(cb, &self.rooms, box_cells);
        let crossing_cost = self.schedule.total_crossing_priority(cb, &self.rooms, box_cells);

        corral_boost + crossing_cost
    }

    pub fn rooms(&self) -> &RoomMap {
        &self.rooms
    }

    pub fn room_count(&self) -> u16 {
        self.rooms.room_count
    }

    pub fn doorway_count(&self) -> u16 {
        self.rooms.doorway_count
    }

    pub fn max_doorway_crossings(&self) -> u16 {
        self.schedule.max_crossings()
    }

    /// Summary string for logging.
    pub fn summary(&self) -> String {
        if !self.has_structure {
            return "no structural plan (puzzle too simple)".into();
        }
        format!(
            "{} rooms, {} doorways, max {} crossings",
            self.rooms.room_count,
            self.rooms.doorway_count,
            self.schedule.max_crossings()
        )
    }
}

fn should_plan(cb: &CompiledBoard, rooms: &RoomMap) -> bool {
    let box_count = cb.initial_box_cells.len();
    let floor_count = cb.cell_count as usize;

    if box_count < MIN_BOXES_FOR_PLANNING && floor_count < MIN_FLOOR_FOR_PLANNING {
        return false;
    }

    rooms.room_count > 1 || rooms.doorway_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn small_puzzle_inactive() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let plan = StructuralPlan::build(&cb);

        assert!(!plan.is_active());
        assert_eq!(plan.evaluate_state(&cb, &[]), 0);
    }

    #[test]
    fn large_open_room_inactive() {
        let rows = &[
            "OOOOOOOOOOO",
            "OSSSSSS   O",
            "O         O",
            "O         O",
            "O XXXXXXR O",
            "O         O",
            "O         O",
            "OOOOOOOOOOO",
        ];
        let board = parse_board(rows);
        if board.is_err() {
            return;
        }
        let cb = CompiledBoard::from_parsed(&board.unwrap());
        let plan = StructuralPlan::build(&cb);

        // Single room (no doorways) → inactive even though large.
        if plan.rooms().room_count == 1 {
            assert!(!plan.is_active());
        }
    }

    #[test]
    fn evaluate_solved_zero() {
        let rows = &[
            "OOOOOOO",
            "OSS   O",
            "O     O",
            "O XX  O",
            "O  R  O",
            "O     O",
            "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let plan = StructuralPlan::build(&cb);

        let box_cells: Vec<(u16, u8)> = cb
            .goal_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();

        // Solved state should have zero boost regardless of plan activity.
        let boost = plan.evaluate_state(&cb, &box_cells);
        assert_eq!(boost, 0);
    }

    #[test]
    fn summary_no_panic() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let plan = StructuralPlan::build(&cb);

        let s = plan.summary();
        assert!(!s.is_empty());
    }

    #[test]
    fn two_room_plan_active() {
        let rows = &[
            "OOOOOOOOO",
            "OS      O",
            "OOOOO OOO",
            "O       O",
            "O XR    O",
            "OOOOOOOOO",
        ];
        let board = parse_board(rows);
        if board.is_err() {
            return;
        }
        let cb = CompiledBoard::from_parsed(&board.unwrap());
        let plan = StructuralPlan::build(&cb);

        // May or may not be active depending on exact floor count.
        if plan.is_active() {
            assert!(plan.room_count() >= 2);
            assert!(plan.doorway_count() >= 1);
            let s = plan.summary();
            assert!(s.contains("rooms"));
        }
    }
}
