use sokomind_core::position::Direction;
use std::collections::VecDeque;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};

/// Compute all cells reachable by the keeper from `start` without moving boxes.
/// `box_cells` is a sorted slice of (cell, label) pairs.
/// Returns a bitset (Vec<bool>) indexed by cell id.
pub fn keeper_reachable(cb: &CompiledBoard, start: u16, box_cells: &[(u16, u8)]) -> Vec<bool> {
    let mut visited = vec![false; cb.cell_count as usize];
    if start == INVALID_CELL {
        return visited;
    }

    let mut queue = VecDeque::new();
    visited[start as usize] = true;
    queue.push_back(start);

    while let Some(cell) = queue.pop_front() {
        for dir in Direction::ALL {
            let n = cb.neighbor(cell, dir);
            if n == INVALID_CELL || visited[n as usize] {
                continue;
            }
            if box_cells.binary_search_by_key(&n, |&(c, _)| c).is_ok() {
                continue;
            }
            visited[n as usize] = true;
            queue.push_back(n);
        }
    }

    visited
}

/// Return the smallest cell id reachable from `start` (canonical representative).
/// Used to normalize keeper position for transposition.
pub fn canonical_keeper(cb: &CompiledBoard, start: u16, box_cells: &[(u16, u8)]) -> u16 {
    let reachable = keeper_reachable(cb, start, box_cells);
    reachable
        .iter()
        .enumerate()
        .filter(|(_, &v)| v)
        .map(|(i, _)| i as u16)
        .min()
        .unwrap_or(start)
}

/// Check if the keeper can reach `target` from `start` without moving boxes.
pub fn can_reach(cb: &CompiledBoard, start: u16, target: u16, box_cells: &[(u16, u8)]) -> bool {
    if start == target {
        return true;
    }
    if start == INVALID_CELL || target == INVALID_CELL {
        return false;
    }

    let mut visited = vec![false; cb.cell_count as usize];
    let mut queue = VecDeque::new();
    visited[start as usize] = true;
    queue.push_back(start);

    while let Some(cell) = queue.pop_front() {
        for dir in Direction::ALL {
            let n = cb.neighbor(cell, dir);
            if n == INVALID_CELL || visited[n as usize] {
                continue;
            }
            if box_cells.binary_search_by_key(&n, |&(c, _)| c).is_ok() {
                continue;
            }
            if n == target {
                return true;
            }
            visited[n as usize] = true;
            queue.push_back(n);
        }
    }

    false
}

/// Find the shortest walk path from `start` to `target`, returning the
/// sequence of directions. Returns None if unreachable.
pub fn find_keeper_path(
    cb: &CompiledBoard,
    start: u16,
    target: u16,
    box_cells: &[(u16, u8)],
) -> Option<Vec<Direction>> {
    if start == target {
        return Some(Vec::new());
    }
    if start == INVALID_CELL || target == INVALID_CELL {
        return None;
    }

    let mut parent: Vec<(u16, Direction)> = vec![(INVALID_CELL, Direction::Up); cb.cell_count as usize];
    let mut visited = vec![false; cb.cell_count as usize];
    let mut queue = VecDeque::new();
    visited[start as usize] = true;
    queue.push_back(start);

    while let Some(cell) = queue.pop_front() {
        for dir in Direction::ALL {
            let n = cb.neighbor(cell, dir);
            if n == INVALID_CELL || visited[n as usize] {
                continue;
            }
            if box_cells.binary_search_by_key(&n, |&(c, _)| c).is_ok() {
                continue;
            }
            visited[n as usize] = true;
            parent[n as usize] = (cell, dir);
            if n == target {
                let mut path = Vec::new();
                let mut cur = target;
                while cur != start {
                    let (prev, d) = parent[cur as usize];
                    path.push(d);
                    cur = prev;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(n);
        }
    }

    None
}

/// Compute shortest keeper path length from `start` to `target`.
/// Returns None if unreachable.
pub fn keeper_distance(
    cb: &CompiledBoard,
    start: u16,
    target: u16,
    box_cells: &[(u16, u8)],
) -> Option<u32> {
    if start == target {
        return Some(0);
    }
    if start == INVALID_CELL || target == INVALID_CELL {
        return None;
    }

    let mut dist = vec![u32::MAX; cb.cell_count as usize];
    let mut queue = VecDeque::new();
    dist[start as usize] = 0;
    queue.push_back(start);

    while let Some(cell) = queue.pop_front() {
        let d = dist[cell as usize];
        for dir in Direction::ALL {
            let n = cb.neighbor(cell, dir);
            if n == INVALID_CELL {
                continue;
            }
            if box_cells.binary_search_by_key(&n, |&(c, _)| c).is_ok() {
                continue;
            }
            if dist[n as usize] <= d + 1 {
                continue;
            }
            dist[n as usize] = d + 1;
            if n == target {
                return Some(d + 1);
            }
            queue.push_back(n);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    fn setup() -> CompiledBoard {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        CompiledBoard::from_parsed(&board)
    }

    #[test]
    fn reachable_without_boxes() {
        let cb = setup();
        let reach = keeper_reachable(&cb, cb.robot_cell, &[]);
        let count = reach.iter().filter(|&&v| v).count();
        assert_eq!(count, cb.cell_count as usize);
    }

    #[test]
    fn box_blocks_path() {
        let cb = setup();
        let box_cell = cb.initial_box_cells[0].0;
        let mut boxes: Vec<(u16, u8)> = vec![(box_cell, 0)];
        boxes.sort();
        let reach = keeper_reachable(&cb, cb.robot_cell, &boxes);
        assert!(!reach[box_cell as usize]);
    }

    #[test]
    fn canonical_is_minimum() {
        let cb = setup();
        let canon = canonical_keeper(&cb, cb.robot_cell, &[]);
        assert!(canon <= cb.robot_cell);
        for c in 0..canon {
            assert_eq!(cb.pos_to_cell(cb.cell_to_pos(c)), c);
        }
    }

    #[test]
    fn can_reach_self() {
        let cb = setup();
        assert!(can_reach(&cb, cb.robot_cell, cb.robot_cell, &[]));
    }

    #[test]
    fn keeper_distance_to_self_is_zero() {
        let cb = setup();
        assert_eq!(
            keeper_distance(&cb, cb.robot_cell, cb.robot_cell, &[]),
            Some(0)
        );
    }

    #[test]
    fn keeper_distance_adjacent() {
        let cb = setup();
        let target = cb.pos_to_cell(Position::new(3, 3));
        let dist = keeper_distance(&cb, cb.robot_cell, target, &[]);
        assert_eq!(dist, Some(1));
    }

    #[test]
    fn keeper_distance_blocked() {
        let cb = setup();
        let box_cell = cb.pos_to_cell(Position::new(1, 1));
        let mut sorted_boxes: Vec<(u16, u8)> = vec![
            (cb.pos_to_cell(Position::new(1, 2)), 0),
            (cb.pos_to_cell(Position::new(2, 2)), 0),
            (cb.pos_to_cell(Position::new(3, 2)), 0),
        ];
        sorted_boxes.sort();
        let dist = keeper_distance(&cb, cb.robot_cell, box_cell, &sorted_boxes);
        assert_eq!(dist, None);
    }
}
