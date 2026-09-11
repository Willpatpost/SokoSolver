use sokomind_core::position::Direction;
use std::collections::VecDeque;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::reachability::{canonical_keeper, keeper_reachable};

const MAX_WINDOW: usize = 8;
const MAX_EXPANDED: usize = 50_000;

/// Try to shorten a push sequence by re-solving overlapping windows.
///
/// For each window of `w` consecutive pushes (w from MAX_WINDOW down to 3),
/// run a bounded BFS from the pre-window state to the post-window box
/// configuration. If a shorter push sub-sequence is found, splice it in and
/// restart from the beginning.
///
/// Returns the number of pushes saved.
pub fn optimize_pushes(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    pushes: &mut Vec<(usize, Direction)>,
) -> u32 {
    let initial_len = pushes.len() as u32;
    let mut improved = true;

    while improved {
        improved = false;
        for window_size in (3..=MAX_WINDOW.min(pushes.len())).rev() {
            let mut offset = 0;
            while offset + window_size <= pushes.len() {
                let pre_state = replay_pushes(cb, &pushes[..offset]);
                let post_state = replay_pushes(cb, &pushes[..offset + window_size]);

                if let Some(shorter) = find_shorter_push_sequence(
                    cb,
                    deadlocks,
                    &pre_state,
                    &post_state.box_cells,
                    window_size as u32 - 1,
                ) {
                    let new_len = shorter.len();
                    pushes.splice(offset..offset + window_size, shorter);
                    offset += new_len;
                    improved = true;
                } else {
                    offset += 1;
                }
            }
        }
    }

    initial_len.saturating_sub(pushes.len() as u32)
}

/// Replay a push sequence from the initial board state, returning the
/// resulting DenseState.
fn replay_pushes(cb: &CompiledBoard, pushes: &[(usize, Direction)]) -> DenseState {
    let mut box_cells: Vec<(u16, u8)> = cb
        .initial_box_cells
        .iter()
        .map(|&(c, l)| (c, l.0))
        .collect();
    box_cells.sort();
    let mut keeper = cb.robot_cell;

    for &(box_index, dir) in pushes {
        let box_cell = box_cells[box_index].0;
        let target = cb.neighbor(box_cell, dir);
        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
        keeper = box_cell;
    }

    let keeper_zone = canonical_keeper(cb, keeper, &box_cells);
    DenseState {
        keeper_zone,
        box_cells,
        moves: 0,
        pushes: 0,
    }
}

/// BFS to find a push sequence from `start_state` that reaches `target_boxes`,
/// with at most `max_pushes` pushes. Returns None if no shorter sequence exists.
fn find_shorter_push_sequence(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    start: &DenseState,
    target_boxes: &[(u16, u8)],
    max_pushes: u32,
) -> Option<Vec<(usize, Direction)>> {
    #[derive(Clone)]
    struct BfsNode {
        keeper_zone: u16,
        box_cells: Vec<(u16, u8)>,
        path: Vec<(usize, Direction)>,
    }

    let mut queue = VecDeque::new();
    let mut visited = rustc_hash::FxHashSet::default();
    let init_key = (start.keeper_zone, start.box_cells.clone());
    visited.insert(init_key);

    queue.push_back(BfsNode {
        keeper_zone: start.keeper_zone,
        box_cells: start.box_cells.clone(),
        path: Vec::new(),
    });

    let mut expanded = 0usize;

    while let Some(node) = queue.pop_front() {
        if node.box_cells == *target_boxes {
            return Some(node.path);
        }

        if node.path.len() as u32 >= max_pushes {
            continue;
        }

        expanded += 1;
        if expanded > MAX_EXPANDED {
            return None;
        }

        let reachable = keeper_reachable(cb, node.keeper_zone, &node.box_cells);

        for (bi, &(box_cell, label)) in node.box_cells.iter().enumerate() {
            for dir in Direction::ALL {
                let target = cb.neighbor(box_cell, dir);
                if target == INVALID_CELL {
                    continue;
                }
                if node
                    .box_cells
                    .binary_search_by_key(&target, |&(c, _)| c)
                    .is_ok()
                {
                    continue;
                }

                let push_from = cb.neighbor(box_cell, dir.opposite());
                if push_from == INVALID_CELL {
                    continue;
                }
                if node
                    .box_cells
                    .binary_search_by_key(&push_from, |&(c, _)| c)
                    .is_ok()
                {
                    continue;
                }
                if !reachable[push_from as usize] {
                    continue;
                }

                let mut new_boxes = node.box_cells.clone();
                new_boxes[bi] = (target, label);
                new_boxes.sort();

                let new_keeper = canonical_keeper(cb, box_cell, &new_boxes);

                let key = (new_keeper, new_boxes.clone());
                if !visited.insert(key) {
                    continue;
                }

                if deadlocks.is_deadlocked_quick(cb, &new_boxes) {
                    continue;
                }

                let mut new_path = node.path.clone();
                new_path.push((bi, dir));

                queue.push_back(BfsNode {
                    keeper_zone: new_keeper,
                    box_cells: new_boxes,
                    path: new_path,
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn already_optimal_unchanged() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);

        // Optimal 2-push solution: up then right.
        let mut pushes = vec![(0, Direction::Up), (0, Direction::Right)];
        let saved = optimize_pushes(&cb, &deadlocks, &mut pushes);
        assert_eq!(saved, 0);
        assert_eq!(pushes.len(), 2);
    }

    #[test]
    fn replay_pushes_correct() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let state = replay_pushes(&cb, &[]);
        assert_eq!(state.box_cells.len(), 1);

        let state2 = replay_pushes(&cb, &[(0, Direction::Up)]);
        assert_ne!(state.box_cells[0].0, state2.box_cells[0].0);
    }

    #[test]
    fn empty_pushes_no_crash() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);

        let mut pushes = Vec::new();
        let saved = optimize_pushes(&cb, &deadlocks, &mut pushes);
        assert_eq!(saved, 0);
    }
}
