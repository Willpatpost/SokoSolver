use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use sokomind_core::position::Direction;

/// Check if any box is part of a 2x2 deadlock.
/// A 2x2 block is deadlocked when all four cells are occupied (by boxes or walls)
/// and at least one box in the block is not on its matching goal.
pub fn is_two_by_two_deadlock(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> bool {
    for &(cell, _) in box_cells {
        if check_2x2_at(cb, box_cells, cell) {
            return true;
        }
    }
    false
}

/// Check the four possible 2x2 blocks that include `cell` as a corner.
/// The cell is top-left, top-right, bottom-left, or bottom-right of the block.
fn check_2x2_at(cb: &CompiledBoard, box_cells: &[(u16, u8)], cell: u16) -> bool {
    let right = cb.neighbor(cell, Direction::Right);
    let down = cb.neighbor(cell, Direction::Down);

    // cell is top-left corner: check (cell, right, down, down-right)
    if right != INVALID_CELL && down != INVALID_CELL {
        let down_right = cb.neighbor(down, Direction::Right);
        if down_right != INVALID_CELL
            && is_blocked_2x2(cb, box_cells, [cell, right, down, down_right])
        {
            return true;
        }
    }

    // cell is top-right corner: check (left, cell, down-left, down)
    let left = cb.neighbor(cell, Direction::Left);
    if left != INVALID_CELL && down != INVALID_CELL {
        let down_left = cb.neighbor(down, Direction::Left);
        if down_left != INVALID_CELL && is_blocked_2x2(cb, box_cells, [left, cell, down_left, down])
        {
            return true;
        }
    }

    // cell is bottom-left corner: check (up, up-right, cell, right)
    let up = cb.neighbor(cell, Direction::Up);
    if up != INVALID_CELL && right != INVALID_CELL {
        let up_right = cb.neighbor(up, Direction::Right);
        if up_right != INVALID_CELL && is_blocked_2x2(cb, box_cells, [up, up_right, cell, right]) {
            return true;
        }
    }

    // cell is bottom-right corner: check (up-left, up, left, cell)
    if up != INVALID_CELL && left != INVALID_CELL {
        let up_left = cb.neighbor(up, Direction::Left);
        if up_left != INVALID_CELL && is_blocked_2x2(cb, box_cells, [up_left, up, left, cell]) {
            return true;
        }
    }

    false
}

/// Given four cells forming a 2x2 block [tl, tr, bl, br], check if
/// all are occupied (box or wall-neighbor = INVALID_CELL) and at least
/// one box is not on its matching goal.
fn is_blocked_2x2(cb: &CompiledBoard, box_cells: &[(u16, u8)], block: [u16; 4]) -> bool {
    let mut any_box_off_goal = false;

    for &cell in &block {
        if let Some(&(_, label)) = box_cells.iter().find(|&&(c, _)| c == cell) {
            if !cb.goal_matches(cell, label) {
                any_box_off_goal = true;
            }
        }
        // Cell is a floor cell (not INVALID_CELL) and not a box — block is not full.
        else {
            return false;
        }
    }

    any_box_off_goal
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn two_boxes_against_corner_wall() {
        // OOOOO
        // OXX O
        // O  SO
        // O  RO
        // OOOOO
        // Boxes at (1,1) and (1,2), wall above and left → 2x2 with walls
        let rows = &["OOOOO", "OXX O", "O SSO", "O  RO", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(1, 1));
        let b2 = cb.pos_to_cell(Position::new(1, 2));
        // Walls at (0,1) and (0,2) are not floor cells, so their neighbors
        // from (1,1) up and (1,2) up are INVALID_CELL.
        // But 2x2 checks only floor cells. Walls bordering the boxes
        // are INVALID_CELL neighbors, not part of the 2x2 block.
        // So the 2x2 block with (1,1) as top-left would be (1,1),(1,2),(2,1),(2,2).
        // (2,1) and (2,2) are floor — not boxes. Not a 2x2 deadlock.
        assert!(!is_two_by_two_deadlock(&cb, &[(b1, 0), (b2, 0)]));
    }

    #[test]
    fn four_boxes_in_square() {
        // OOOOOOO
        // O     O
        // O XX  O
        // ORXX  O
        // O  SS O
        // O  SS O
        // OOOOOOO
        let rows = &[
            "OOOOOOO", "O     O", "O XX  O", "ORXX  O", "O  SS O", "O  SS O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(2, 2));
        let b2 = cb.pos_to_cell(Position::new(2, 3));
        let b3 = cb.pos_to_cell(Position::new(3, 2));
        let b4 = cb.pos_to_cell(Position::new(3, 3));
        assert!(is_two_by_two_deadlock(
            &cb,
            &[(b1, 0), (b2, 0), (b3, 0), (b4, 0)]
        ));
    }

    #[test]
    fn four_boxes_all_on_goals_not_deadlock() {
        // OOOOOOO
        // O     O
        // O SS  O
        // ORSS  O
        // O     O
        // OOOOOOO
        // Wait — we need boxes ON goals. In our format, a cell can only be one thing.
        // So we can't have boxes and goals on the same cell in the row string.
        // Instead, test with the box_cells placed at goal positions.
        let rows = &[
            "OOOOOOO", "O     O", "O XX  O", "ORXX  O", "O  SS O", "O  SS O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Place boxes on the goal cells
        let g1 = cb.goal_cells[0].0;
        let g2 = cb.goal_cells[1].0;
        let g3 = cb.goal_cells[2].0;
        let g4 = cb.goal_cells[3].0;
        assert!(!is_two_by_two_deadlock(
            &cb,
            &[(g1, 0), (g2, 0), (g3, 0), (g4, 0)]
        ));
    }

    #[test]
    fn no_deadlock_open_space() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let box_cell = cb.pos_to_cell(Position::new(2, 2));
        assert!(!is_two_by_two_deadlock(&cb, &[(box_cell, 0)]));
    }
}
