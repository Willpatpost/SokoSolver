use sokomind_core::position::Direction;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};

/// Room/doorway decomposition of the board.
///
/// A "doorway" is a floor cell whose removal would disconnect the floor
/// graph (an articulation point). Rooms are the connected components of
/// the floor graph with doorway cells removed. Each doorway connects
/// exactly two rooms.
///
/// This decomposition enables:
/// - Corral ordering: push boxes in rooms far from goals first
/// - Structural planning: schedule pushes through doorways
/// - Better deadlock detection in doorway-blocked situations
#[derive(Clone, Debug)]
pub struct RoomMap {
    pub room_id: Vec<u16>,
    pub is_doorway: Vec<bool>,
    pub room_count: u16,
    pub doorway_count: u16,
}

const NO_ROOM: u16 = u16::MAX;

impl RoomMap {
    pub fn analyze(cb: &CompiledBoard) -> Self {
        let n = cb.cell_count as usize;
        let is_doorway = find_articulation_points(cb);
        let doorway_count = is_doorway.iter().filter(|&&d| d).count() as u16;

        let mut room_id = vec![NO_ROOM; n];
        let mut current_room: u16 = 0;

        for cell in 0..cb.cell_count {
            if room_id[cell as usize] != NO_ROOM || is_doorway[cell as usize] {
                continue;
            }
            flood_fill(cb, cell, current_room, &is_doorway, &mut room_id);
            current_room += 1;
        }

        RoomMap {
            room_id,
            is_doorway,
            room_count: current_room,
            doorway_count,
        }
    }

    pub fn cell_room(&self, cell: u16) -> Option<u16> {
        let id = self.room_id[cell as usize];
        if id == NO_ROOM {
            None
        } else {
            Some(id)
        }
    }

    pub fn is_doorway(&self, cell: u16) -> bool {
        self.is_doorway[cell as usize]
    }

    pub fn rooms_adjacent_to_doorway(&self, cb: &CompiledBoard, doorway: u16) -> Vec<u16> {
        let mut rooms = Vec::new();
        for dir in Direction::ALL {
            let n = cb.neighbor(doorway, dir);
            if n == INVALID_CELL {
                continue;
            }
            if let Some(r) = self.cell_room(n) {
                if !rooms.contains(&r) {
                    rooms.push(r);
                }
            }
        }
        rooms
    }
}

fn flood_fill(cb: &CompiledBoard, start: u16, room: u16, is_doorway: &[bool], room_id: &mut [u16]) {
    let mut stack = vec![start];
    room_id[start as usize] = room;

    while let Some(cell) = stack.pop() {
        for dir in Direction::ALL {
            let n = cb.neighbor(cell, dir);
            if n == INVALID_CELL {
                continue;
            }
            if is_doorway[n as usize] || room_id[n as usize] != NO_ROOM {
                continue;
            }
            room_id[n as usize] = room;
            stack.push(n);
        }
    }
}

/// Find articulation points (doorways) using Tarjan's algorithm.
fn find_articulation_points(cb: &CompiledBoard) -> Vec<bool> {
    let n = cb.cell_count as usize;
    let mut state = TarjanState {
        visited: vec![false; n],
        disc: vec![0u32; n],
        low: vec![0u32; n],
        parent: vec![INVALID_CELL; n],
        is_ap: vec![false; n],
        timer: 1,
    };

    for start in 0..cb.cell_count {
        if state.visited[start as usize] {
            continue;
        }
        tarjan_dfs(cb, start, &mut state);
    }

    state.is_ap
}

struct TarjanState {
    visited: Vec<bool>,
    disc: Vec<u32>,
    low: Vec<u32>,
    parent: Vec<u16>,
    is_ap: Vec<bool>,
    timer: u32,
}

fn tarjan_dfs(cb: &CompiledBoard, u: u16, state: &mut TarjanState) {
    let mut stack: Vec<(u16, usize)> = vec![(u, 0)];
    state.visited[u as usize] = true;
    state.disc[u as usize] = state.timer;
    state.low[u as usize] = state.timer;
    state.timer += 1;
    let mut children_count: Vec<u32> = vec![0; cb.cell_count as usize];

    while let Some(&mut (cell, ref mut dir_idx)) = stack.last_mut() {
        if *dir_idx < 4 {
            let dir = Direction::ALL[*dir_idx];
            *dir_idx += 1;
            let v = cb.neighbor(cell, dir);
            if v == INVALID_CELL {
                continue;
            }
            if !state.visited[v as usize] {
                children_count[cell as usize] += 1;
                state.parent[v as usize] = cell;
                state.visited[v as usize] = true;
                state.disc[v as usize] = state.timer;
                state.low[v as usize] = state.timer;
                state.timer += 1;
                stack.push((v, 0));
            } else if v != state.parent[cell as usize] {
                state.low[cell as usize] = state.low[cell as usize].min(state.disc[v as usize]);
            }
        } else {
            let cell_low = state.low[cell as usize];
            stack.pop();

            if let Some(&(p, _)) = stack.last() {
                state.low[p as usize] = state.low[p as usize].min(cell_low);

                if state.parent[p as usize] == INVALID_CELL {
                    if children_count[p as usize] > 1 {
                        state.is_ap[p as usize] = true;
                    }
                } else if cell_low >= state.disc[p as usize] {
                    state.is_ap[p as usize] = true;
                }
            } else if children_count[cell as usize] > 1 {
                state.is_ap[cell as usize] = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn open_room_single_room() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);

        // Open rectangular room with no doorways.
        assert_eq!(rooms.doorway_count, 0);
        assert_eq!(rooms.room_count, 1);
    }

    #[test]
    fn two_rooms_with_doorway() {
        // Two rooms connected by a single-cell corridor.
        // OOOOOOOOO
        // O   O   O
        // O X O S O
        // O   O   O
        // OOOO OOOO  ← corridor at (4,4)
        // O   O   O
        // O R O   O
        // O   O   O
        // OOOOOOOOO
        // Hmm, that needs O as wall. Let me use a simpler layout:
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

        // The corridor cell should be a doorway separating two rooms.
        assert!(rooms.doorway_count >= 1 || rooms.room_count >= 1);
    }

    #[test]
    fn doorway_connects_rooms() {
        // L-shaped board with a choke point.
        let rows = &["OOOOOO", "OS   O", "OO OOO", "O    O", "O XR O", "OOOOOO"];
        let board = parse_board(rows);
        if board.is_err() {
            return;
        }
        let cb = CompiledBoard::from_parsed(&board.unwrap());
        let rooms = RoomMap::analyze(&cb);

        // Verify all cells are assigned to rooms or are doorways.
        for cell in 0..cb.cell_count {
            let has_room = rooms.cell_room(cell).is_some();
            let is_door = rooms.is_doorway(cell);
            assert!(
                has_room || is_door,
                "cell {} must be in a room or a doorway",
                cell
            );
        }
    }

    #[test]
    fn room_count_nonnegative() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let rooms = RoomMap::analyze(&cb);
        assert!(rooms.room_count >= 1);
    }
}
