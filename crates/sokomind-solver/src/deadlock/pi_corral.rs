use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::reachability::keeper_reachable;
use sokomind_core::position::Direction;

/// Pi-corral deadlock detector.
///
/// A pi-corral is a region enclosed by walls that the keeper cannot
/// enter. If any off-goal box is completely isolated — the keeper
/// cannot reach ANY adjacent cell — the box can never be pushed and
/// the state is deadlocked.
///
/// This is conservative: a box blocked only by other movable boxes is
/// NOT flagged, since those boxes might be pushed out of the way first.
/// Only boxes where every adjacent cell is either a wall or unreachable
/// (with no keeper path even ignoring push targets) are detected.
pub fn is_pi_corral_deadlock(cb: &CompiledBoard, keeper_pos: u16, box_cells: &[(u16, u8)]) -> bool {
    if box_cells.is_empty() || keeper_pos == INVALID_CELL {
        return false;
    }

    let reachable = keeper_reachable(cb, keeper_pos, box_cells);

    for &(box_cell, label) in box_cells {
        if cb.goal_matches(box_cell, label) {
            continue;
        }

        let mut any_adjacent_reachable = false;
        for dir in Direction::ALL {
            let adj = cb.neighbor(box_cell, dir);
            if adj == INVALID_CELL {
                continue;
            }
            if box_cells.binary_search_by_key(&adj, |&(c, _)| c).is_ok() {
                continue;
            }
            if reachable[adj as usize] {
                any_adjacent_reachable = true;
                break;
            }
        }

        if !any_adjacent_reachable {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn no_corral_open_board() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let keeper = cb.robot_cell;
        let box_cell = cb.initial_box_cells[0].0;
        let label = cb.initial_box_cells[0].1 .0;
        assert!(!is_pi_corral_deadlock(&cb, keeper, &[(box_cell, label)]));
    }

    #[test]
    fn box_behind_boxes_not_pi_corral() {
        // Box at (1,2) has a keeper-reachable adjacent cell (1,3), so it's
        // not in a true pi-corral even though it can't be pushed right now.
        // The goal-commitment detector catches this instead.
        let rows = &[
            "OOOOOOO", "O X   O", "OXXXO O", "O SSS O", "OR  S O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let keeper = cb.robot_cell;
        let mut boxes: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        boxes.sort();

        assert!(!is_pi_corral_deadlock(&cb, keeper, &boxes));
    }

    #[test]
    fn box_fully_enclosed_by_walls_and_boxes() {
        // Box at (1,1): adjacent cells are (0,1)=wall, (1,0)=wall,
        // (2,1)=wall, (1,2)=box. Keeper cannot reach any non-box
        // adjacent cell → true pi-corral.
        let rows = &["OOOOOO", "OXX  O", "OO S O", "O  R O", "O S  O", "OOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let keeper = cb.robot_cell;
        let mut boxes: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        boxes.sort();

        assert!(is_pi_corral_deadlock(&cb, keeper, &boxes));
    }

    #[test]
    fn all_boxes_reachable() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let keeper = cb.robot_cell;
        let mut boxes: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        boxes.sort();

        assert!(!is_pi_corral_deadlock(&cb, keeper, &boxes));
    }

    #[test]
    fn trapped_box_on_goal_not_deadlock() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let goal_cell = cb.goal_cells[0].0;
        let goal_label = cb.goal_cells[0].1 .0;
        // Even if unreachable, a box on its matching goal is fine.
        assert!(!is_pi_corral_deadlock(
            &cb,
            cb.robot_cell,
            &[(goal_cell, goal_label)]
        ));
    }

    #[test]
    fn empty_boxes() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        assert!(!is_pi_corral_deadlock(&cb, cb.robot_cell, &[]));
    }
}
