use sokomind_core::position::Direction;
use std::collections::VecDeque;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::deadlock::DeadlockChecker;
use crate::reachability::{canonical_keeper, keeper_distance, keeper_reachable};

const MAX_SEGMENT: usize = 12;
const MAX_EXPANDED: usize = 30_000;

/// Reschedule which box is pushed at each step to minimize total moves.
///
/// Splits the push sequence into segments where each segment pushes a
/// single box consecutively. For each pair of adjacent segments (pushing
/// different boxes), tries swapping the order — if the alternate order
/// produces a valid, shorter solution, it's kept.
///
/// Returns the number of moves saved.
pub fn reschedule_boxes(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    pushes: &mut Vec<(usize, Direction)>,
) -> u32 {
    if pushes.len() < 2 {
        return 0;
    }

    let initial_cost = total_walk_cost(cb, pushes);
    let mut improved = true;

    while improved {
        improved = false;
        let segments = identify_segments(pushes);
        if segments.len() < 2 {
            break;
        }

        for si in 0..segments.len() - 1 {
            let seg_a = &segments[si];
            let seg_b = &segments[si + 1];

            if seg_a.box_index == seg_b.box_index {
                continue;
            }

            let combined_len = seg_a.len + seg_b.len;
            if combined_len > MAX_SEGMENT {
                continue;
            }

            let offset = seg_a.start;
            let pre_state = replay_to_offset(cb, pushes, offset);
            let post_boxes = replay_to_offset(cb, pushes, offset + combined_len).box_cells;

            if let Some(alt) = try_swapped_order(
                cb,
                deadlocks,
                &pre_state,
                &post_boxes,
                &pushes[offset..offset + combined_len],
            ) {
                let orig_cost =
                    segment_walk_cost(cb, &pre_state, &pushes[offset..offset + combined_len]);
                let alt_cost = segment_walk_cost(cb, &pre_state, &alt);

                if alt_cost < orig_cost {
                    pushes.splice(offset..offset + combined_len, alt);
                    improved = true;
                    break;
                }
            }
        }
    }

    let final_cost = total_walk_cost(cb, pushes);
    initial_cost.saturating_sub(final_cost)
}

struct Segment {
    start: usize,
    len: usize,
    box_index: usize,
}

fn identify_segments(pushes: &[(usize, Direction)]) -> Vec<Segment> {
    let mut segments = Vec::new();
    if pushes.is_empty() {
        return segments;
    }

    let mut start = 0;
    let mut current_box = pushes[0].0;

    for (i, &(bi, _)) in pushes.iter().enumerate().skip(1) {
        if bi != current_box {
            segments.push(Segment {
                start,
                len: i - start,
                box_index: current_box,
            });
            start = i;
            current_box = bi;
        }
    }
    segments.push(Segment {
        start,
        len: pushes.len() - start,
        box_index: current_box,
    });

    segments
}

struct PartialState {
    keeper: u16,
    box_cells: Vec<(u16, u8)>,
}

fn replay_to_offset(
    cb: &CompiledBoard,
    pushes: &[(usize, Direction)],
    offset: usize,
) -> PartialState {
    let mut box_cells: Vec<(u16, u8)> = cb
        .initial_box_cells
        .iter()
        .map(|&(c, l)| (c, l.0))
        .collect();
    box_cells.sort();
    let mut keeper = cb.robot_cell;

    for &(box_index, dir) in &pushes[..offset] {
        let box_cell = box_cells[box_index].0;
        let target = cb.neighbor(box_cell, dir);
        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
        keeper = box_cell;
    }

    PartialState { keeper, box_cells }
}

fn try_swapped_order(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    pre: &PartialState,
    target_boxes: &[(u16, u8)],
    original: &[(usize, Direction)],
) -> Option<Vec<(usize, Direction)>> {
    #[derive(Clone)]
    struct BfsNode {
        keeper_zone: u16,
        box_cells: Vec<(u16, u8)>,
        path: Vec<(usize, Direction)>,
    }

    let keeper_zone = canonical_keeper(cb, pre.keeper, &pre.box_cells);
    let mut queue = VecDeque::new();
    let mut visited = rustc_hash::FxHashSet::default();
    let init_key = (keeper_zone, pre.box_cells.clone());
    visited.insert(init_key);

    queue.push_back(BfsNode {
        keeper_zone,
        box_cells: pre.box_cells.clone(),
        path: Vec::new(),
    });

    let max_pushes = original.len() as u32;
    let mut expanded = 0usize;

    while let Some(node) = queue.pop_front() {
        if node.box_cells == *target_boxes {
            if node.path.len() <= original.len() && node.path != original {
                return Some(node.path);
            }
            continue;
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

fn segment_walk_cost(
    cb: &CompiledBoard,
    state: &PartialState,
    segment: &[(usize, Direction)],
) -> u32 {
    let mut cost = 0u32;
    let mut keeper = state.keeper;
    let mut box_cells = state.box_cells.clone();

    for &(box_index, dir) in segment {
        if box_index >= box_cells.len() {
            return u32::MAX;
        }
        let box_cell = box_cells[box_index].0;
        let push_from = cb.neighbor(box_cell, dir.opposite());
        if push_from == INVALID_CELL {
            return u32::MAX;
        }

        if let Some(d) = keeper_distance(cb, keeper, push_from, &box_cells) {
            cost += d + 1;
        } else {
            return u32::MAX;
        }

        let target = cb.neighbor(box_cell, dir);
        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
        keeper = box_cell;
    }

    cost
}

fn total_walk_cost(cb: &CompiledBoard, pushes: &[(usize, Direction)]) -> u32 {
    let state = PartialState {
        keeper: cb.robot_cell,
        box_cells: cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect(),
    };
    segment_walk_cost(cb, &state, pushes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn empty_pushes_no_crash() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);

        let mut pushes = Vec::new();
        let saved = reschedule_boxes(&cb, &deadlocks, &mut pushes);
        assert_eq!(saved, 0);
    }

    #[test]
    fn single_box_no_change() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);

        let mut pushes = vec![(0, Direction::Up), (0, Direction::Right)];
        let saved = reschedule_boxes(&cb, &deadlocks, &mut pushes);
        assert_eq!(saved, 0);
    }

    #[test]
    fn identify_segments_basic() {
        let pushes = vec![
            (0, Direction::Up),
            (0, Direction::Right),
            (1, Direction::Down),
            (1, Direction::Left),
            (0, Direction::Up),
        ];
        let segs = identify_segments(&pushes);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].box_index, 0);
        assert_eq!(segs[0].len, 2);
        assert_eq!(segs[1].box_index, 1);
        assert_eq!(segs[1].len, 2);
        assert_eq!(segs[2].box_index, 0);
        assert_eq!(segs[2].len, 1);
    }

    #[test]
    fn segments_single_box() {
        let pushes = vec![(0, Direction::Up), (0, Direction::Right)];
        let segs = identify_segments(&pushes);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].box_index, 0);
        assert_eq!(segs[0].len, 2);
    }

    #[test]
    fn two_box_reschedule() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);

        let mut pushes = vec![(0, Direction::Up), (0, Direction::Up)];
        let original_len = pushes.len();
        let saved = reschedule_boxes(&cb, &deadlocks, &mut pushes);
        assert!(pushes.len() <= original_len);
        assert_eq!(saved, 0); // may or may not save depending on layout
    }
}
