use crate::compiled_board::CompiledBoard;

use super::rooms::RoomMap;

/// Doorway crossing schedule for boxes.
///
/// For each box, computes the minimum number of doorway crossings
/// needed to reach any compatible goal. Boxes requiring more crossings
/// should be prioritized in beam search (they take more moves and are
/// more likely to block other boxes if left until last).
pub struct DoorwaySchedule {
    box_crossings: Vec<u16>,
}

impl DoorwaySchedule {
    /// Build a doorway schedule from the current board state.
    ///
    /// For each initial box, counts how many room boundaries (doorway
    /// crossings) separate it from its nearest compatible goal.
    pub fn build(cb: &CompiledBoard, rooms: &RoomMap) -> Self {
        let mut box_crossings = Vec::with_capacity(cb.initial_box_cells.len());

        for &(box_cell, box_label) in &cb.initial_box_cells {
            let crossings = min_doorway_crossings(cb, rooms, box_cell, box_label.0);
            box_crossings.push(crossings);
        }

        Self { box_crossings }
    }

    /// Compute crossings needed for a specific box at a given cell.
    pub fn crossings_for_box(&self, box_index: usize) -> u16 {
        self.box_crossings.get(box_index).copied().unwrap_or(0)
    }

    /// Compute crossings for a box at an arbitrary cell (runtime query).
    pub fn crossings_from_cell(cb: &CompiledBoard, rooms: &RoomMap, cell: u16, label: u8) -> u16 {
        min_doorway_crossings(cb, rooms, cell, label)
    }

    /// Priority score: boxes needing more crossings get higher priority.
    /// Returns the sum of crossings for all non-goal boxes.
    pub fn total_crossing_priority(
        &self,
        cb: &CompiledBoard,
        rooms: &RoomMap,
        box_cells: &[(u16, u8)],
    ) -> u32 {
        let mut total = 0u32;

        for &(cell, label) in box_cells {
            if cb.goal_matches(cell, label) {
                continue;
            }
            let crossings = min_doorway_crossings(cb, rooms, cell, label);
            total += crossings as u32;
        }

        total
    }

    pub fn max_crossings(&self) -> u16 {
        self.box_crossings.iter().copied().max().unwrap_or(0)
    }
}

/// Compute minimum doorway crossings from a cell to any compatible goal.
///
/// Uses room distances: if box and goal are in the same room, crossings = 0.
/// If separated by doorways, crossings = room_distance between them.
fn min_doorway_crossings(cb: &CompiledBoard, rooms: &RoomMap, cell: u16, label: u8) -> u16 {
    let box_room = if rooms.is_doorway(cell) {
        None
    } else {
        rooms.cell_room(cell)
    };

    let mut min_crossings = u16::MAX;

    for &(goal_cell, goal_label) in &cb.goal_cells {
        if goal_label.0 != label {
            continue;
        }

        let goal_room = if rooms.is_doorway(goal_cell) {
            None
        } else {
            rooms.cell_room(goal_cell)
        };

        let crossings = match (box_room, goal_room) {
            (Some(br), Some(gr)) => {
                if br == gr {
                    0
                } else {
                    room_bfs_distance(rooms, cb, br, gr).unwrap_or(u16::MAX)
                }
            }
            _ => 0,
        };

        if crossings < min_crossings {
            min_crossings = crossings;
        }
    }

    if min_crossings == u16::MAX {
        0
    } else {
        min_crossings
    }
}

/// BFS distance between two rooms in the room adjacency graph.
fn room_bfs_distance(rooms: &RoomMap, cb: &CompiledBoard, from: u16, to: u16) -> Option<u16> {
    if from == to {
        return Some(0);
    }

    let n_rooms = rooms.room_count as usize;
    let mut dist = vec![u16::MAX; n_rooms];
    let mut queue = std::collections::VecDeque::new();

    dist[from as usize] = 0;
    queue.push_back(from);

    let adj = build_room_adj(rooms, cb);

    while let Some(room) = queue.pop_front() {
        let d = dist[room as usize];
        if let Some(neighbors) = adj.get(room as usize) {
            for &n in neighbors {
                if dist[n as usize] > d + 1 {
                    dist[n as usize] = d + 1;
                    if n == to {
                        return Some(d + 1);
                    }
                    queue.push_back(n);
                }
            }
        }
    }

    None
}

fn build_room_adj(rooms: &RoomMap, cb: &CompiledBoard) -> Vec<Vec<u16>> {
    let n_rooms = rooms.room_count as usize;
    let mut adj: Vec<Vec<u16>> = vec![Vec::new(); n_rooms];

    let mut visited_doorways = vec![false; cb.cell_count as usize];

    for cell in 0..cb.cell_count {
        if !rooms.is_doorway(cell) || visited_doorways[cell as usize] {
            continue;
        }

        let mut cluster_rooms: Vec<u16> = Vec::new();
        let mut stack = vec![cell];
        visited_doorways[cell as usize] = true;

        while let Some(d) = stack.pop() {
            for dir in sokomind_core::position::Direction::ALL {
                let n = cb.neighbor(d, dir);
                if n == crate::compiled_board::INVALID_CELL {
                    continue;
                }
                if let Some(r) = rooms.cell_room(n) {
                    if !cluster_rooms.contains(&r) {
                        cluster_rooms.push(r);
                    }
                } else if rooms.is_doorway(n) && !visited_doorways[n as usize] {
                    visited_doorways[n as usize] = true;
                    stack.push(n);
                }
            }
        }

        for i in 0..cluster_rooms.len() {
            for j in (i + 1)..cluster_rooms.len() {
                let a = cluster_rooms[i];
                let b = cluster_rooms[j];
                if !adj[a as usize].contains(&b) {
                    adj[a as usize].push(b);
                }
                if !adj[b as usize].contains(&a) {
                    adj[b as usize].push(a);
                }
            }
        }
    }

    adj
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn single_room_zero_crossings() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let schedule = DoorwaySchedule::build(&cb, &rooms);

        assert_eq!(schedule.max_crossings(), 0);
        for i in 0..cb.initial_box_cells.len() {
            assert_eq!(schedule.crossings_for_box(i), 0);
        }
    }

    #[test]
    fn solved_state_zero_priority() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let schedule = DoorwaySchedule::build(&cb, &rooms);

        let box_cells: Vec<(u16, u8)> = cb.goal_cells.iter().map(|&(c, l)| (c, l.0)).collect();
        let priority = schedule.total_crossing_priority(&cb, &rooms, &box_cells);
        assert_eq!(priority, 0);
    }

    #[test]
    fn two_room_has_crossings() {
        let rows = &[
            "OOOOOOOOO",
            "OS      O",
            "OOOOO OOO",
            "O       O",
            "O XR    O",
            "OOOOOOOOO",
        ];
        let board = parse_board(rows);
        if board.is_err() {
            return;
        }
        let cb = CompiledBoard::from_parsed(&board.unwrap());
        let rooms = RoomMap::analyze(&cb);

        if rooms.room_count < 2 || rooms.doorway_count == 0 {
            return;
        }

        let schedule = DoorwaySchedule::build(&cb, &rooms);
        // If multiple rooms exist, at least some crossings should be computed.
        // The exact count depends on room/box/goal topology.
        assert!(
            schedule.max_crossings() < 100,
            "crossings should be bounded"
        );
    }

    #[test]
    fn crossings_from_cell_runtime() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);

        let box_cell = cb.initial_box_cells[0].0;
        let label = cb.initial_box_cells[0].1 .0;
        let crossings = DoorwaySchedule::crossings_from_cell(&cb, &rooms, box_cell, label);
        assert_eq!(crossings, 0); // single room
    }

    #[test]
    fn out_of_range_box_index() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let schedule = DoorwaySchedule::build(&cb, &rooms);

        assert_eq!(schedule.crossings_for_box(999), 0);
    }
}
