use sokomind_core::position::Direction;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::reachability::keeper_distance;

const MAX_WINDOW: usize = 6;

/// Try reordering pushes within small windows to minimize total walk distance.
///
/// For each window of consecutive pushes, enumerate all permutations (up to
/// MAX_WINDOW! = 720), simulate each one, and keep the permutation that
/// produces the fewest total moves (walks + pushes).
///
/// Returns the number of moves saved.
pub fn optimize_permutations(cb: &CompiledBoard, pushes: &mut Vec<(usize, Direction)>) -> u32 {
    if pushes.len() < 2 {
        return 0;
    }

    let initial_moves = count_total_moves(cb, pushes);
    let mut improved = true;

    while improved {
        improved = false;
        let window_size = MAX_WINDOW.min(pushes.len());

        for ws in (2..=window_size).rev() {
            let mut offset = 0;
            while offset + ws <= pushes.len() {
                let window = &pushes[offset..offset + ws];
                let pre_state = replay_to_offset(cb, pushes, offset);

                if let Some(better) = find_best_permutation(cb, &pre_state, window) {
                    pushes.splice(offset..offset + ws, better);
                    improved = true;
                }
                offset += 1;
            }
        }
    }

    let final_moves = count_total_moves(cb, pushes);
    initial_moves.saturating_sub(final_moves)
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

/// Simulate a push sequence and return the total move count (walks + pushes),
/// or None if any push is illegal.
fn simulate_moves(
    cb: &CompiledBoard,
    state: &PartialState,
    sequence: &[(usize, Direction)],
) -> Option<u32> {
    let mut moves = 0u32;
    let mut keeper = state.keeper;
    let mut box_cells = state.box_cells.clone();

    for &(box_index, dir) in sequence {
        if box_index >= box_cells.len() {
            return None;
        }
        let box_cell = box_cells[box_index].0;
        let push_from = cb.neighbor(box_cell, dir.opposite());
        if push_from == INVALID_CELL {
            return None;
        }

        let target = cb.neighbor(box_cell, dir);
        if target == INVALID_CELL {
            return None;
        }
        if box_cells.binary_search_by_key(&target, |&(c, _)| c).is_ok() {
            return None;
        }
        if box_cells
            .binary_search_by_key(&push_from, |&(c, _)| c)
            .is_ok()
        {
            return None;
        }

        let walk_dist = keeper_distance(cb, keeper, push_from, &box_cells)?;
        moves += walk_dist + 1;

        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
        keeper = box_cell;
    }

    Some(moves)
}

/// Try all permutations of the window and return the best one if it's
/// strictly better than the original.
fn find_best_permutation(
    cb: &CompiledBoard,
    state: &PartialState,
    window: &[(usize, Direction)],
) -> Option<Vec<(usize, Direction)>> {
    let n = window.len();
    let original_cost = simulate_moves(cb, state, window)?;

    let mut best_cost = original_cost;
    let mut best_perm: Option<Vec<(usize, Direction)>> = None;

    let mut indices: Vec<usize> = (0..n).collect();
    let mut perm_buf: Vec<(usize, Direction)> = vec![(0, Direction::Up); n];

    // Heap's algorithm for generating permutations.
    let mut c = vec![0usize; n];
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 {
                indices.swap(0, i);
            } else {
                indices.swap(c[i], i);
            }

            for (j, &idx) in indices.iter().enumerate() {
                perm_buf[j] = window[idx];
            }

            // Check that the final box configuration matches the original.
            if let Some(cost) = simulate_moves(cb, state, &perm_buf) {
                let perm_final = simulate_final_boxes(cb, state, &perm_buf);
                let orig_final = simulate_final_boxes(cb, state, window);
                if perm_final == orig_final && cost < best_cost {
                    best_cost = cost;
                    best_perm = Some(perm_buf.clone());
                }
            }

            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }

    best_perm
}

fn simulate_final_boxes(
    cb: &CompiledBoard,
    state: &PartialState,
    sequence: &[(usize, Direction)],
) -> Vec<(u16, u8)> {
    let mut box_cells = state.box_cells.clone();
    for &(box_index, dir) in sequence {
        if box_index >= box_cells.len() {
            return box_cells;
        }
        let box_cell = box_cells[box_index].0;
        let target = cb.neighbor(box_cell, dir);
        if target == INVALID_CELL {
            return box_cells;
        }
        let label = box_cells[box_index].1;
        box_cells[box_index] = (target, label);
        box_cells.sort();
    }
    box_cells
}

fn count_total_moves(cb: &CompiledBoard, pushes: &[(usize, Direction)]) -> u32 {
    let state = PartialState {
        keeper: cb.robot_cell,
        box_cells: cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect(),
    };
    simulate_moves(cb, &state, pushes).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn single_push_no_change() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let mut pushes = vec![(0, Direction::Up)];
        let saved = optimize_permutations(&cb, &mut pushes);
        assert_eq!(saved, 0);
    }

    #[test]
    fn empty_pushes_no_crash() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let mut pushes = Vec::new();
        let saved = optimize_permutations(&cb, &mut pushes);
        assert_eq!(saved, 0);
    }

    #[test]
    fn simulate_moves_basic() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let state = PartialState {
            keeper: cb.robot_cell,
            box_cells: cb
                .initial_box_cells
                .iter()
                .map(|&(c, l)| (c, l.0))
                .collect(),
        };

        let result = simulate_moves(&cb, &state, &[(0, Direction::Left)]);
        // Robot at (2,3), box at (2,2), push left from (2,3).
        // Walk 0 steps + 1 push = some value.
        assert!(result.is_some());
    }
}
