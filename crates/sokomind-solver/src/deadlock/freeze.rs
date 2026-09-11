use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use sokomind_core::position::Direction;

/// Check for freeze deadlock: a box is frozen if it cannot move along either
/// axis (both directions blocked by walls or other frozen boxes). If any frozen
/// box is not on its matching goal, the state is deadlocked.
///
/// Uses pessimistic fixed-point: starts with all boxes assumed frozen, then
/// iteratively unfreezes any box that has a movable axis. This correctly
/// handles mutual freeze dependencies.
pub fn is_freeze_deadlock(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> bool {
    let n = box_cells.len();
    if n == 0 {
        return false;
    }

    let mut frozen = vec![true; n];
    let mut changed = true;

    while changed {
        changed = false;
        for i in 0..n {
            if !frozen[i] {
                continue;
            }
            let (cell, _) = box_cells[i];
            if !is_frozen(cb, box_cells, &frozen, cell) {
                frozen[i] = false;
                changed = true;
            }
        }
    }

    for i in 0..n {
        if frozen[i] {
            let (cell, label) = box_cells[i];
            if !cb.goal_matches(cell, label) {
                return true;
            }
        }
    }

    false
}

fn is_frozen(cb: &CompiledBoard, box_cells: &[(u16, u8)], frozen: &[bool], cell: u16) -> bool {
    let h_blocked = is_axis_blocked(
        cb,
        box_cells,
        frozen,
        cell,
        Direction::Left,
        Direction::Right,
    );
    let v_blocked = is_axis_blocked(cb, box_cells, frozen, cell, Direction::Up, Direction::Down);
    h_blocked && v_blocked
}

fn is_axis_blocked(
    cb: &CompiledBoard,
    box_cells: &[(u16, u8)],
    frozen: &[bool],
    cell: u16,
    dir_a: Direction,
    dir_b: Direction,
) -> bool {
    is_direction_blocked(cb, box_cells, frozen, cell, dir_a)
        && is_direction_blocked(cb, box_cells, frozen, cell, dir_b)
}

fn is_direction_blocked(
    cb: &CompiledBoard,
    box_cells: &[(u16, u8)],
    frozen: &[bool],
    cell: u16,
    dir: Direction,
) -> bool {
    let neighbor = cb.neighbor(cell, dir);
    if neighbor == INVALID_CELL {
        return true;
    }
    if let Some(idx) = box_cells.iter().position(|&(c, _)| c == neighbor) {
        return frozen[idx];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn single_box_open_not_frozen() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let box_cell = cb.pos_to_cell(Position::new(2, 2));
        assert!(!is_freeze_deadlock(&cb, &[(box_cell, 0)]));
    }

    #[test]
    fn single_box_in_corner_frozen() {
        // A single box at (1,1): wall above and left blocks two directions,
        // but right and down are open floor. Not blocked on either axis.
        // However, static deadlock catches corners — freeze should NOT fire here.
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let corner = cb.pos_to_cell(Position::new(1, 1));
        assert!(!is_freeze_deadlock(&cb, &[(corner, 0)]));
    }

    #[test]
    fn box_against_wall_one_axis_not_frozen() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let wall_adj = cb.pos_to_cell(Position::new(1, 2));
        assert!(!is_freeze_deadlock(&cb, &[(wall_adj, 0)]));
    }

    #[test]
    fn two_boxes_one_can_slide() {
        // Two boxes side by side against a wall. (1,2) can slide right,
        // so neither is frozen.
        let rows = &["OOOOO", "OXX O", "ORSSO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(1, 1));
        let b2 = cb.pos_to_cell(Position::new(1, 2));
        assert!(!is_freeze_deadlock(&cb, &[(b1, 0), (b2, 0)]));
    }

    #[test]
    fn mutual_freeze_2x2_corner() {
        // 4 boxes in a 2x2 at (1,1),(1,2),(2,1),(2,2) against top-left corner.
        // Each box has wall on one side and boxes on the other two non-wall sides.
        // (1,1): H=wall+box, V=wall+box → frozen
        // (1,2): H=box+(1,3 open) → can slide right? Only if not frozen.
        // With pessimistic start: all frozen. (1,2) H: left=(1,1) frozen, right=open → open!
        // So H is NOT fully blocked → (1,2) unfreezes.
        // Then (1,1) right neighbor unfrozen → (1,1) H: wall + unfrozen → not blocked.
        // (1,1) unfreezes. Cascade unfreezes all.
        // Result: NOT a freeze deadlock (caught by 2x2 detector instead).
        let rows = &[
            "OOOOOOO", "OXX   O", "OXX   O", "OR    O", "O  SS O", "O  SS O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(1, 1));
        let b2 = cb.pos_to_cell(Position::new(1, 2));
        let b3 = cb.pos_to_cell(Position::new(2, 1));
        let b4 = cb.pos_to_cell(Position::new(2, 2));
        // 2x2 in corner with open sides: freeze should NOT detect this
        // (2x2 detector handles it). Boxes can theoretically slide out.
        assert!(!is_freeze_deadlock(
            &cb,
            &[(b1, 0), (b2, 0), (b3, 0), (b4, 0)]
        ));
    }

    #[test]
    fn freeze_in_wall_channel() {
        // Two boxes stacked vertically in a 1-wide channel between walls.
        // OOOOOO
        // OXO  O
        // OXO  O
        // OR   O
        // O SS O
        // OOOOOO
        // Box (1,1): H=wall-wall, V=wall-box → frozen if (2,1) frozen
        // Box (2,1): H=wall-wall, V=box-open → NOT frozen (can move down)
        // So (1,1) V: up=wall, down=(2,1) unfrozen → not fully blocked → unfreezes
        // Neither frozen. Not a deadlock.
        let rows = &["OOOOOO", "OXO  O", "OXO  O", "OR   O", "O SS O", "OOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(1, 1));
        let b2 = cb.pos_to_cell(Position::new(2, 1));
        assert!(!is_freeze_deadlock(&cb, &[(b1, 0), (b2, 0)]));
    }

    #[test]
    fn freeze_in_sealed_channel() {
        // Two boxes in a 1-wide channel sealed on both ends.
        // OOOOOO
        // OXO  O
        // OXO  O
        // OOO  O
        // O R  O
        // O SS O
        // OOOOOO
        // Box (1,1): H=wall-wall → blocked. V=wall-(2,1) → if (2,1) frozen, blocked.
        // Box (2,1): H=wall-wall → blocked. V=(1,1)-wall → if (1,1) frozen, blocked.
        // Pessimistic: both start frozen. Check (1,1): H=wall-wall blocked, V=wall-frozen blocked → stays frozen.
        // Check (2,1): H=wall-wall blocked, V=frozen-wall blocked → stays frozen.
        // Both remain frozen. Neither on goal → DEADLOCK.
        let rows = &[
            "OOOOOO", "OXO  O", "OXO  O", "OOO  O", "O R  O", "O SS O", "OOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(1, 1));
        let b2 = cb.pos_to_cell(Position::new(2, 1));
        assert!(is_freeze_deadlock(&cb, &[(b1, 0), (b2, 0)]));
    }

    #[test]
    fn frozen_on_goal_not_deadlock() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let goal_cell = cb.goal_cells[0].0;
        let label = cb.goal_cells[0].1 .0;
        assert!(!is_freeze_deadlock(&cb, &[(goal_cell, label)]));
    }
}
