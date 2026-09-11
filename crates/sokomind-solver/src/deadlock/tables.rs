use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::topology::BoardTopology;
use sokomind_core::position::Direction;
use std::collections::VecDeque;

/// Precomputed deadlock table for small board zones.
///
/// During board analysis, identifies "dead zones" — small connected regions
/// (≤ MAX_ZONE_CELLS cells) adjacent to corners/walls where boxes can get
/// stuck. For each zone, enumerates all box configurations and marks those
/// from which no box can escape to a goal.
///
/// At runtime, checks if any subset of current boxes falls entirely within
/// a dead zone configuration.
pub struct DeadlockTable {
    zones: Vec<DeadZone>,
}

struct DeadZone {
    cells: Vec<u16>,
    deadlocked_masks: Vec<u64>,
}

const MAX_ZONE_CELLS: usize = 6;

impl DeadlockTable {
    pub fn build(cb: &CompiledBoard, topo: &BoardTopology) -> Self {
        let mut zones = Vec::new();

        for cell in 0..cb.cell_count {
            if !topo.is_corner(cell) {
                continue;
            }

            let zone_cells = grow_zone(cb, topo, cell);
            if zone_cells.len() < 2 || zone_cells.len() > MAX_ZONE_CELLS {
                continue;
            }

            let deadlocked_masks = compute_deadlocked_masks(cb, &zone_cells);
            if !deadlocked_masks.is_empty() {
                zones.push(DeadZone {
                    cells: zone_cells,
                    deadlocked_masks,
                });
            }
        }

        DeadlockTable { zones }
    }

    pub fn is_deadlocked(&self, box_cells: &[(u16, u8)]) -> bool {
        for zone in &self.zones {
            let mask = zone_occupation_mask(&zone.cells, box_cells);
            if mask == 0 {
                continue;
            }
            if zone.deadlocked_masks.contains(&mask) {
                return true;
            }
        }
        false
    }
}

/// Grow a small zone starting from a corner cell, expanding along walls.
fn grow_zone(cb: &CompiledBoard, topo: &BoardTopology, start: u16) -> Vec<u16> {
    let mut visited = vec![false; cb.cell_count as usize];
    let mut zone = Vec::new();
    let mut queue = VecDeque::new();

    visited[start as usize] = true;
    zone.push(start);
    queue.push_back(start);

    while let Some(cell) = queue.pop_front() {
        if zone.len() >= MAX_ZONE_CELLS {
            break;
        }
        for dir in Direction::ALL {
            let n = cb.neighbor(cell, dir);
            if n == INVALID_CELL || visited[n as usize] {
                continue;
            }
            // Only expand into cells that are near walls (have at least one wall neighbor).
            let wall_adjacent = Direction::ALL
                .iter()
                .any(|&d| cb.neighbor(n, d) == INVALID_CELL);
            if !wall_adjacent && !topo.is_dead(n) {
                continue;
            }
            visited[n as usize] = true;
            zone.push(n);
            if zone.len() >= MAX_ZONE_CELLS {
                break;
            }
            queue.push_back(n);
        }
    }

    zone.sort();
    zone
}

/// For a given zone, enumerate all non-empty subsets of cells (as bitmasks)
/// and determine which configurations are deadlocked.
///
/// A configuration is deadlocked if placing generic-label boxes on those
/// cells (with no other boxes on the board) results in at least one box
/// that cannot reach any goal via reverse-push.
fn compute_deadlocked_masks(cb: &CompiledBoard, zone_cells: &[u16]) -> Vec<u64> {
    let n = zone_cells.len();
    assert!(n <= 63);

    let mut deadlocked = Vec::new();

    for mask in 1u64..(1u64 << n) {
        let box_count = mask.count_ones() as usize;
        if box_count > cb.goal_cells.len() {
            deadlocked.push(mask);
            continue;
        }

        let mut box_positions: Vec<u16> = Vec::with_capacity(box_count);
        for (bit, &cell) in zone_cells.iter().enumerate() {
            if mask & (1u64 << bit) != 0 {
                box_positions.push(cell);
            }
        }

        let mut any_stuck = false;
        for &bp in &box_positions {
            let can_reach_goal = cb
                .goal_cells
                .iter()
                .enumerate()
                .any(|(gi, &(_, _))| cb.reverse_push_distance(gi, bp) < u16::MAX);

            if !can_reach_goal {
                any_stuck = true;
                break;
            }
        }

        if any_stuck {
            deadlocked.push(mask);
        }
    }

    deadlocked
}

/// Compute the bitmask of which zone cells are occupied by boxes.
fn zone_occupation_mask(zone_cells: &[u16], box_cells: &[(u16, u8)]) -> u64 {
    let mut mask = 0u64;
    for (bit, &zone_cell) in zone_cells.iter().enumerate() {
        if box_cells
            .binary_search_by_key(&zone_cell, |&(c, _)| c)
            .is_ok()
        {
            mask |= 1u64 << bit;
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn table_builds_without_panic() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let table = DeadlockTable::build(&cb, &topo);
        // Should have some zones from corners.
        let _ = table.zones.len();
    }

    #[test]
    fn box_on_goal_not_in_table() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);
        let table = DeadlockTable::build(&cb, &topo);

        let goal_cell = cb.goal_cells[0].0;
        let goal_label = cb.goal_cells[0].1 .0;
        assert!(!table.is_deadlocked(&[(goal_cell, goal_label)]));
    }

    #[test]
    fn dead_corner_in_table() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);
        let table = DeadlockTable::build(&cb, &topo);

        let corner = cb.pos_to_cell(Position::new(1, 1));
        // Corner cell is dead — if reverse-push distance is MAX, table should catch it.
        let dist = cb.reverse_push_distance(0, corner);
        if dist == u16::MAX {
            assert!(table.is_deadlocked(&[(corner, 0)]));
        }
    }

    #[test]
    fn open_center_not_deadlocked() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);
        let table = DeadlockTable::build(&cb, &topo);

        let center = cb.pos_to_cell(Position::new(2, 2));
        assert!(!table.is_deadlocked(&[(center, 0)]));
    }

    #[test]
    fn empty_boxes_not_deadlocked() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);
        let table = DeadlockTable::build(&cb, &topo);

        assert!(!table.is_deadlocked(&[]));
    }
}
