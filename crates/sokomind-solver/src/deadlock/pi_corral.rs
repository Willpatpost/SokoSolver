use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::reachability::keeper_reachable;
use sokomind_core::position::Direction;

/// Pi-corral deadlock detector.
///
/// A pi-corral is a region enclosed by walls and boxes that the keeper
/// cannot enter. If any box inside such a region is not on a matching
/// goal, the state is deadlocked — the keeper can never reach a push
/// position for those boxes.
///
/// Algorithm:
/// 1. Compute keeper-reachable cells from `keeper_pos`.
/// 2. Find all boxes the keeper cannot reach from any adjacent cell.
/// 3. For each unreachable box not on its matching goal → deadlock.
///
/// This is a conservative check: it only fires when a box is completely
/// enclosed and off-goal. It does not check whether the enclosed boxes
/// *could* reach goals if they were pushable — that's handled by the
/// goal commitment detector.
pub fn is_pi_corral_deadlock(cb: &CompiledBoard, keeper_pos: u16, box_cells: &[(u16, u8)]) -> bool {
    if box_cells.is_empty() || keeper_pos == INVALID_CELL {
        return false;
    }

    let reachable = keeper_reachable(cb, keeper_pos, box_cells);

    for &(box_cell, label) in box_cells {
        if cb.goal_matches(box_cell, label) {
            continue;
        }

        // A box is pushable if the keeper can reach at least one push position:
        // an adjacent cell from which pushing is possible (the opposite side is open).
        let mut pushable = false;
        for dir in Direction::ALL {
            let push_from = cb.neighbor(box_cell, dir.opposite());
            if push_from == INVALID_CELL {
                continue;
            }
            if box_cells
                .binary_search_by_key(&push_from, |&(c, _)| c)
                .is_ok()
            {
                continue;
            }
            if !reachable[push_from as usize] {
                continue;
            }

            let target = cb.neighbor(box_cell, dir);
            if target == INVALID_CELL {
                continue;
            }
            if box_cells.binary_search_by_key(&target, |&(c, _)| c).is_ok() {
                continue;
            }

            pushable = true;
            break;
        }

        if !pushable {
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
    fn box_trapped_behind_boxes() {
        // Keeper at bottom, boxes forming a wall the keeper can't get past.
        // OOOOOOO
        // O X   O  ← box at (1,2), goal nowhere near it
        // OXXXO O  ← boxes at (2,1),(2,2),(2,3) block passage
        // O SSS O
        // OR  S O
        // OOOOOOO
        // 4 boxes, 4 goals. Box at (1,2) trapped behind the row.
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

        let result = is_pi_corral_deadlock(&cb, keeper, &boxes);
        // The trapped box at (1,2) is not on a goal → deadlock
        assert!(result);
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
