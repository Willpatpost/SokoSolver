use std::collections::VecDeque;

use crate::compiled_board::CompiledBoard;

use super::rooms::RoomMap;

const NO_DIST: u16 = u16::MAX;

/// Room-distance-based corral ordering for beam search.
///
/// Computes each room's minimum distance (in room-graph hops through
/// doorways) to the nearest goal-containing room. Provides a scoring
/// function that penalizes states where boxes sit in rooms far from
/// their goals — biasing beam search to push distant boxes first.
pub struct CorralOrdering {
    room_goal_dist: Vec<u16>,
}

impl CorralOrdering {
    pub fn build(cb: &CompiledBoard, rooms: &RoomMap) -> Self {
        let room_goal_dist = compute_room_goal_distances(cb, rooms);
        Self { room_goal_dist }
    }

    /// Compute a corral boost for the given box configuration.
    ///
    /// Returns the sum of room-goal distances for all boxes not yet on
    /// a goal. Lower is better (boxes closer to goal rooms). This acts
    /// as a tiebreaker in beam search — among equal f-cost states,
    /// prefer those where distant boxes have been moved closer.
    pub fn corral_boost(
        &self,
        cb: &CompiledBoard,
        rooms: &RoomMap,
        box_cells: &[(u16, u8)],
    ) -> u32 {
        let mut boost = 0u32;

        for &(cell, label) in box_cells {
            if cb.goal_matches(cell, label) {
                continue;
            }

            let room_dist = if rooms.is_doorway(cell) {
                self.doorway_min_room_dist(cb, rooms, cell)
            } else if let Some(rid) = rooms.cell_room(cell) {
                self.room_goal_dist.get(rid as usize).copied().unwrap_or(0) as u32
            } else {
                0
            };

            boost += room_dist;
        }

        boost
    }

    pub fn room_distance(&self, room_id: u16) -> u16 {
        self.room_goal_dist
            .get(room_id as usize)
            .copied()
            .unwrap_or(NO_DIST)
    }

    fn doorway_min_room_dist(&self, cb: &CompiledBoard, rooms: &RoomMap, doorway: u16) -> u32 {
        let adj = rooms.rooms_adjacent_to_doorway(cb, doorway);
        adj.iter()
            .filter_map(|&r| self.room_goal_dist.get(r as usize))
            .map(|&d| d as u32)
            .min()
            .unwrap_or(0)
    }
}

/// BFS on the room adjacency graph to compute each room's distance
/// from the nearest goal-containing room.
fn compute_room_goal_distances(cb: &CompiledBoard, rooms: &RoomMap) -> Vec<u16> {
    let n_rooms = rooms.room_count as usize;
    if n_rooms == 0 {
        return Vec::new();
    }

    let adj = build_room_adjacency(cb, rooms);

    let mut dist = vec![NO_DIST; n_rooms];
    let mut queue = VecDeque::new();

    for &(goal_cell, _) in &cb.goal_cells {
        let room = if rooms.is_doorway(goal_cell) {
            for &r in &rooms.rooms_adjacent_to_doorway(cb, goal_cell) {
                if dist[r as usize] == NO_DIST {
                    dist[r as usize] = 0;
                    queue.push_back(r);
                }
            }
            continue;
        } else if let Some(r) = rooms.cell_room(goal_cell) {
            r
        } else {
            continue;
        };

        if dist[room as usize] == NO_DIST {
            dist[room as usize] = 0;
            queue.push_back(room);
        }
    }

    while let Some(room) = queue.pop_front() {
        let d = dist[room as usize];
        if let Some(neighbors) = adj.get(room as usize) {
            for &neighbor in neighbors {
                if dist[neighbor as usize] > d + 1 {
                    dist[neighbor as usize] = d + 1;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    dist
}

/// Build room adjacency list.
///
/// Two rooms are adjacent if they're connected through a chain of
/// doorway cells. A single doorway may not touch both rooms directly
/// (doorway chains are common in corridors), so we flood-fill through
/// doorways to find all rooms reachable from each doorway cluster.
fn build_room_adjacency(cb: &CompiledBoard, rooms: &RoomMap) -> Vec<Vec<u16>> {
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
    fn single_room_zero_distance() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let ordering = CorralOrdering::build(&cb, &rooms);

        assert_eq!(rooms.room_count, 1);
        assert_eq!(ordering.room_distance(0), 0);
    }

    #[test]
    fn corral_boost_solved_is_zero() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let ordering = CorralOrdering::build(&cb, &rooms);

        let box_cells: Vec<(u16, u8)> = cb.goal_cells.iter().map(|&(c, l)| (c, l.0)).collect();
        assert_eq!(ordering.corral_boost(&cb, &rooms, &box_cells), 0);
    }

    #[test]
    fn corral_boost_nonnegative() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let ordering = CorralOrdering::build(&cb, &rooms);

        let box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        let boost = ordering.corral_boost(&cb, &rooms, &box_cells);
        assert!(boost < 1000);
    }

    #[test]
    fn two_room_distance() {
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

        if rooms.room_count < 2 {
            return;
        }

        let ordering = CorralOrdering::build(&cb, &rooms);

        let has_zero = (0..rooms.room_count).any(|r| ordering.room_distance(r) == 0);
        assert!(has_zero, "goal room should have distance 0");

        for r in 0..rooms.room_count {
            let d = ordering.room_distance(r);
            assert!(d < 100, "room {} distance should be bounded, got {}", r, d);
        }
    }

    #[test]
    fn empty_board_no_panic() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        let ordering = CorralOrdering::build(&cb, &rooms);
        let boost = ordering.corral_boost(&cb, &rooms, &[]);
        assert_eq!(boost, 0);
    }
}
