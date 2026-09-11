use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use sokomind_core::position::Direction;

/// Precomputed safe-goal information.
///
/// A "safe goal" is a goal cell where a correctly-labeled box, once placed,
/// should never be moved again. This happens when the goal is in a corner
/// or dead-end — pushing the box away would make it impossible to return.
///
/// During successor generation, boxes on safe goals are skipped, reducing
/// the branching factor.
#[derive(Clone, Debug)]
pub struct SafeGoalMap {
    safe: Vec<bool>,
}

impl SafeGoalMap {
    pub fn analyze(cb: &CompiledBoard) -> Self {
        let n = cb.cell_count as usize;
        let mut safe = vec![false; n];

        for &(goal_cell, _) in &cb.goal_cells {
            if is_safe_goal(cb, goal_cell) {
                safe[goal_cell as usize] = true;
            }
        }

        SafeGoalMap { safe }
    }

    /// Returns true if a box with matching label on this goal should not
    /// be moved. The caller must verify label matching separately.
    pub fn is_safe(&self, cell: u16) -> bool {
        self.safe.get(cell as usize).copied().unwrap_or(false)
    }

    /// Check if a box at `cell` with `label` is on a safe goal and should
    /// be treated as committed (skip generating pushes for it).
    pub fn is_committed(&self, cb: &CompiledBoard, cell: u16, label: u8) -> bool {
        cb.goal_matches(cell, label) && self.is_safe(cell)
    }
}

/// A goal is safe if it's in a corner (walls on two adjacent sides).
/// Pushing a box off a corner goal means it can never return.
fn is_safe_goal(cb: &CompiledBoard, cell: u16) -> bool {
    let up = cb.neighbor(cell, Direction::Up) == INVALID_CELL;
    let down = cb.neighbor(cell, Direction::Down) == INVALID_CELL;
    let left = cb.neighbor(cell, Direction::Left) == INVALID_CELL;
    let right = cb.neighbor(cell, Direction::Right) == INVALID_CELL;

    (up && left) || (up && right) || (down && left) || (down && right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn corner_goal_is_safe() {
        // Goal at (1,1) — corner with walls above and left.
        let rows = &["OOOOO", "OS  O", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let sgm = SafeGoalMap::analyze(&cb);

        let goal_cell = cb.pos_to_cell(Position::new(1, 1));
        assert!(sgm.is_safe(goal_cell));
    }

    #[test]
    fn mid_wall_goal_not_safe() {
        // Goal at (2,3) — wall to the right but open on left, up, and down.
        // Not a corner → not safe.
        let rows = &[
            "OOOOOOO", "O     O", "O  S  O", "O XR  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let sgm = SafeGoalMap::analyze(&cb);

        let goal_cell = cb.pos_to_cell(Position::new(2, 3));
        assert!(!sgm.is_safe(goal_cell));
    }

    #[test]
    fn center_goal_not_safe() {
        let rows = &[
            "OOOOOOO", "O     O", "O  S  O", "O XR  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let sgm = SafeGoalMap::analyze(&cb);

        let goal_cell = cb.pos_to_cell(Position::new(2, 3));
        assert!(!sgm.is_safe(goal_cell));
    }

    #[test]
    fn committed_check() {
        let rows = &["OOOOO", "OS  O", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let sgm = SafeGoalMap::analyze(&cb);

        let goal_cell = cb.pos_to_cell(Position::new(1, 1));
        let goal_label = cb.goal_cells[0].1 .0;

        // Matching label on safe goal → committed.
        assert!(sgm.is_committed(&cb, goal_cell, goal_label));

        // Wrong label → not committed.
        assert!(!sgm.is_committed(&cb, goal_cell, goal_label + 1));

        // Correct label but not on goal → not committed.
        let other_cell = cb.pos_to_cell(Position::new(2, 2));
        assert!(!sgm.is_committed(&cb, other_cell, goal_label));
    }
}
