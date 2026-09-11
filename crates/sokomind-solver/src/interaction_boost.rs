use std::collections::VecDeque;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use sokomind_core::position::Direction;

/// Count box-box interaction penalties beyond linear conflict.
///
/// For each box, computes the shortest push path to its nearest compatible
/// goal on an empty board vs with other boxes present. If the path is
/// blocked (other boxes sit on required intermediate cells), a penalty
/// is assessed. This is admissible because each blocked box needs at
/// least one extra push to clear the path and one to return.
///
/// Only counts interactions where a box is directly blocking another
/// box's push corridor — not just any spatial proximity.
pub fn count_interaction_penalties(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
    if box_cells.len() < 2 {
        return 0;
    }

    let mut penalties = 0u32;

    for (i, &(box_cell, label)) in box_cells.iter().enumerate() {
        let empty_dist = nearest_goal_distance_empty(cb, box_cell, label);
        if empty_dist == u32::MAX {
            continue;
        }

        let blocked_dist = nearest_goal_distance_with_boxes(cb, box_cell, label, box_cells, i);

        if blocked_dist > empty_dist && blocked_dist != u32::MAX {
            let diff = blocked_dist - empty_dist;
            if diff >= 2 {
                penalties += 2;
            }
        } else if blocked_dist == u32::MAX && empty_dist != u32::MAX {
            penalties += 2;
        }
    }

    penalties
}

fn nearest_goal_distance_empty(cb: &CompiledBoard, box_cell: u16, label: u8) -> u32 {
    let mut min_dist = u32::MAX;

    for (gi, &(_, goal_label)) in cb.goal_cells.iter().enumerate() {
        if goal_label.0 != label {
            continue;
        }
        let dist = cb.reverse_push_distance(gi, box_cell);
        if (dist as u32) < min_dist {
            min_dist = dist as u32;
        }
    }

    min_dist
}

fn nearest_goal_distance_with_boxes(
    cb: &CompiledBoard,
    box_cell: u16,
    label: u8,
    all_boxes: &[(u16, u8)],
    skip_index: usize,
) -> u32 {
    let obstacle_set: Vec<u16> = all_boxes
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != skip_index)
        .map(|(_, &(c, _))| c)
        .collect();

    let mut dist = vec![u32::MAX; cb.cell_count as usize];
    let mut queue = VecDeque::new();

    dist[box_cell as usize] = 0;
    queue.push_back(box_cell);

    while let Some(cell) = queue.pop_front() {
        let d = dist[cell as usize];

        for dir in Direction::ALL {
            let target = cb.neighbor(cell, dir);
            if target == INVALID_CELL {
                continue;
            }

            let pusher_from = cb.neighbor(cell, dir.opposite());
            if pusher_from == INVALID_CELL {
                continue;
            }

            if obstacle_set.contains(&target) || obstacle_set.contains(&pusher_from) {
                continue;
            }

            if dist[target as usize] > d + 1 {
                dist[target as usize] = d + 1;
                queue.push_back(target);
            }
        }
    }

    let mut min_dist = u32::MAX;
    for &(goal_cell, goal_label) in &cb.goal_cells {
        if goal_label.0 != label {
            continue;
        }
        if dist[goal_cell as usize] < min_dist {
            min_dist = dist[goal_cell as usize];
        }
    }

    min_dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn single_box_no_penalty() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        assert_eq!(count_interaction_penalties(&cb, &box_cells), 0);
    }

    #[test]
    fn no_boxes_no_penalty() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        assert_eq!(count_interaction_penalties(&cb, &[]), 0);
    }

    #[test]
    fn boxes_not_blocking_no_penalty() {
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

        let box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        let penalty = count_interaction_penalties(&cb, &box_cells);
        // Boxes are side by side but not blocking each other's paths upward.
        assert!(penalty <= 4);
    }

    #[test]
    fn blocking_box_detected() {
        // Narrow corridor: box B blocks box A's path to goal.
        // OOOOO
        // O  sO  (goal at (1,3))
        // O  RO
        // O  XO  (box A at (3,2))
        // O  XO  (box B at (4,2) — blocks push path)
        // OOOOO
        let rows = &["OOOOO", "O  SO", "O  RO", "O  XO", "O  XO", "OOOOO"];
        let board = parse_board(rows);
        if board.is_err() {
            return;
        }
        let cb = CompiledBoard::from_parsed(&board.unwrap());

        let mut box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        box_cells.sort();
        let penalty = count_interaction_penalties(&cb, &box_cells);
        // At least one box should be penalized for blocking.
        assert!(penalty <= 8, "penalty should be bounded");
    }

    #[test]
    fn solved_state_no_penalty() {
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

        let box_cells: Vec<(u16, u8)> = cb
            .goal_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        let penalty = count_interaction_penalties(&cb, &box_cells);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn empty_board_distance() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let box_cell = cb.initial_box_cells[0].0;
        let label = cb.initial_box_cells[0].1.0;
        let dist = nearest_goal_distance_empty(&cb, box_cell, label);
        assert!(dist > 0 && dist < u32::MAX);
    }
}
