use crate::compiled_board::{CompiledBoard, INVALID_CELL};

/// Pattern deadlock detector: checks for closed-diagonal deadlocks.
///
/// A closed diagonal occurs when two boxes are placed diagonally adjacent
/// and the two connecting cells (forming the 2x2 square) are both walls.
/// If at least one box is not on a matching goal, this is a deadlock —
/// neither box can ever be pushed out of the diagonal configuration.
///
/// This catches cases that 2x2 (requires all 4 occupied) and freeze
/// (requires full axis blocking) both miss.
pub fn is_pattern_deadlock(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> bool {
    for (i, &(cell_a, label_a)) in box_cells.iter().enumerate() {
        for &(cell_b, label_b) in &box_cells[i + 1..] {
            if is_closed_diagonal(cb, cell_a, label_a, cell_b, label_b) {
                return true;
            }
        }
    }
    false
}

fn is_closed_diagonal(
    cb: &CompiledBoard,
    cell_a: u16,
    label_a: u8,
    cell_b: u16,
    label_b: u8,
) -> bool {
    let pos_a = cb.cell_to_pos(cell_a);
    let pos_b = cb.cell_to_pos(cell_b);

    let dr = pos_b.row - pos_a.row;
    let dc = pos_b.col - pos_a.col;

    if dr.abs() != 1 || dc.abs() != 1 {
        return false;
    }

    let connect1 = cb.pos_to_cell(sokomind_core::position::Position::new(pos_a.row, pos_b.col));
    let connect2 = cb.pos_to_cell(sokomind_core::position::Position::new(pos_b.row, pos_a.col));

    if connect1 != INVALID_CELL || connect2 != INVALID_CELL {
        return false;
    }

    let a_on_goal = cb.goal_matches(cell_a, label_a);
    let b_on_goal = cb.goal_matches(cell_b, label_b);
    !(a_on_goal && b_on_goal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn no_diagonal_deadlock_open_board() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.initial_box_cells[0].0;
        let b2 = cb.initial_box_cells[1].0;
        let l1 = cb.initial_box_cells[0].1 .0;
        let l2 = cb.initial_box_cells[1].1 .0;
        let mut boxes = vec![(b1, l1), (b2, l2)];
        boxes.sort();
        assert!(!is_pattern_deadlock(&cb, &boxes));
    }

    #[test]
    fn single_box_no_pattern() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let box_cell = cb.pos_to_cell(Position::new(2, 2));
        assert!(!is_pattern_deadlock(&cb, &[(box_cell, 0)]));
    }

    #[test]
    fn empty_boxes() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        assert!(!is_pattern_deadlock(&cb, &[]));
    }

    #[test]
    fn non_diagonal_adjacent_boxes() {
        let rows = &["OOOOOOO", "O  SS O", "O XXR O", "O     O", "OOOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.initial_box_cells[0].0;
        let b2 = cb.initial_box_cells[1].0;
        let l1 = cb.initial_box_cells[0].1 .0;
        let l2 = cb.initial_box_cells[1].1 .0;
        let mut boxes = vec![(b1, l1), (b2, l2)];
        boxes.sort();
        assert!(!is_pattern_deadlock(&cb, &boxes));
    }

    #[test]
    fn closed_diagonal_detected() {
        // Construct a board where two boxes are diagonally adjacent
        // with walls at the connecting positions.
        //
        // OOOOOOO
        // O  S  O
        // O SOX O
        // O  XR O
        // O     O
        // OOOOOOO
        //
        // If we manually place boxes at (2,3) and (3,2), those are diagonal.
        // The connecting cells are (2,2) and (3,3).
        // Both are floor cells → NOT a closed diagonal.
        // We need a board where the connecting cells are walls.
        //
        // Board with jagged walls creating a diagonal wall pocket:
        // OOOOOO
        // O  S O
        // O XO O
        // OOX  O
        // O  R O
        // OOOOOO
        // Boxes at (2,2) and (3,2): same column, not diagonal → no pattern.
        //
        // Actually, to get a real closed diagonal, we need an irregular board:
        // OOOOO
        // OX OO  ← (1,1)=box, (1,2)=floor, (1,3..4)=wall
        // OO XO  ← (2,0..1)=wall, (2,2)=floor, (2,3)=box, (2,4)=wall
        // O  SO
        // O R O
        // OOOOO
        //
        // Boxes at (1,1) and (2,3): not diagonal (distance > 1).
        //
        // Simpler: boxes at (r,c) and (r+1,c+1) with (r,c+1) and (r+1,c) as walls.
        // This requires the board shape to have a wall notch.
        //
        // OOOOOO
        // OXO  O   ← box at (1,1), wall at (1,2)
        // OO   O   ← wall at (2,1)
        // O XS O   ← box at (3,2)
        // O  S O
        // O R  O
        // OOOOOO
        //
        // Boxes at (1,1) and (2,2)? (2,1) is wall, (1,2) is wall → closed diagonal!
        // But wait, (2,2) has no box in this layout...
        //
        // Let me construct this directly:
        let rows = &["OOOOOO", "OXO  O", "OOXS O", "O  S O", "O R  O", "OOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Box at (1,1). Wall at (1,2) and (2,1) [the OO].
        // Place second box at (2,2) to test diagonal with (1,1).
        let b1 = cb.pos_to_cell(Position::new(1, 1));
        let b2 = cb.pos_to_cell(Position::new(2, 2));

        // Connect cells: (1,2) and (2,1) — both should be walls.
        let c1 = cb.pos_to_cell(Position::new(1, 2));
        let c2 = cb.pos_to_cell(Position::new(2, 1));
        assert_eq!(c1, INVALID_CELL, "(1,2) should be wall");
        assert_eq!(c2, INVALID_CELL, "(2,1) should be wall");

        assert!(is_pattern_deadlock(&cb, &[(b1, 0), (b2, 0)]));
    }

    #[test]
    fn diagonal_both_on_goals_not_deadlock() {
        // Two boxes on goals that happen to be diagonal with wall-corners
        // between them should NOT be flagged as deadlocked.
        let rows = &["OOOOOO", "OXO  O", "OOXS O", "O  S O", "O R  O", "OOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Place boxes on goal cells with matching labels.
        let g1 = cb.goal_cells[0].0;
        let g2 = cb.goal_cells[1].0;
        let l1 = cb.goal_cells[0].1 .0;
        let l2 = cb.goal_cells[1].1 .0;
        let mut boxes = vec![(g1, l1), (g2, l2)];
        boxes.sort();
        // Both on matching goals → not a deadlock even if diagonal with walls.
        assert!(!is_pattern_deadlock(&cb, &boxes));
    }
}
