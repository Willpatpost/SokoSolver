use rustc_hash::FxHashSet;
use sokomind_core::board::ParsedBoard;
use sokomind_core::position::{Direction, Position};
use sokomind_core::types::Label;
use std::collections::VecDeque;

pub const INVALID_CELL: u16 = u16::MAX;

#[derive(Clone, Debug)]
pub struct CompiledBoard {
    pub cell_count: u16,
    positions: Vec<Position>,
    cell_index: Vec<Vec<u16>>,
    neighbors: Vec<[u16; 4]>,
    pub goal_cells: Vec<(u16, Label)>,
    pub wall_set: FxHashSet<Position>,
    pub width: u16,
    pub height: u16,
    pub robot_cell: u16,
    pub initial_box_cells: Vec<(u16, Label)>,
    goal_at: Vec<Option<u8>>,
    reverse_push_dist: Vec<u16>,
}

impl CompiledBoard {
    pub fn from_parsed(board: &ParsedBoard) -> Self {
        let width = board.width;
        let height = board.height;

        let mut wall_set = FxHashSet::default();
        for &w in &board.walls {
            wall_set.insert(w);
        }

        let mut cell_index = vec![vec![INVALID_CELL; width as usize]; height as usize];
        let mut positions = Vec::new();
        let mut cell_id: u16 = 0;

        for &pos in &board.floor {
            cell_index[pos.row as usize][pos.col as usize] = cell_id;
            positions.push(pos);
            cell_id += 1;
        }

        let cell_count = cell_id;

        let mut neighbors = vec![[INVALID_CELL; 4]; cell_count as usize];
        for c in 0..cell_count {
            let pos = positions[c as usize];
            for dir in Direction::ALL {
                let adj = pos.offset(dir);
                if adj.row >= 0 && adj.row < height as i16 && adj.col >= 0 && adj.col < width as i16
                {
                    let n = cell_index[adj.row as usize][adj.col as usize];
                    neighbors[c as usize][dir as usize] = n;
                }
            }
        }

        let robot_cell =
            cell_index[board.initial_robot.row as usize][board.initial_robot.col as usize];

        let initial_box_cells: Vec<(u16, Label)> = board
            .initial_boxes
            .iter()
            .map(|b| {
                (
                    cell_index[b.position.row as usize][b.position.col as usize],
                    b.label,
                )
            })
            .collect();

        let goal_cells: Vec<(u16, Label)> = board
            .goals
            .iter()
            .map(|g| {
                (
                    cell_index[g.position.row as usize][g.position.col as usize],
                    g.label,
                )
            })
            .collect();

        let mut goal_at = vec![None; cell_count as usize];
        for &(cell, label) in &goal_cells {
            goal_at[cell as usize] = Some(label.0);
        }

        let reverse_push_dist =
            Self::compute_reverse_push_distances(cell_count, &neighbors, &goal_cells);

        CompiledBoard {
            cell_count,
            positions,
            cell_index,
            neighbors,
            goal_cells,
            wall_set,
            width,
            height,
            robot_cell,
            initial_box_cells,
            goal_at,
            reverse_push_dist,
        }
    }

    pub fn pos_to_cell(&self, pos: Position) -> u16 {
        if pos.row < 0
            || pos.col < 0
            || pos.row >= self.height as i16
            || pos.col >= self.width as i16
        {
            return INVALID_CELL;
        }
        self.cell_index[pos.row as usize][pos.col as usize]
    }

    pub fn cell_to_pos(&self, cell: u16) -> Position {
        self.positions[cell as usize]
    }

    pub fn neighbor(&self, cell: u16, dir: Direction) -> u16 {
        self.neighbors[cell as usize][dir as usize]
    }

    pub fn is_goal(&self, cell: u16) -> bool {
        self.goal_at[cell as usize].is_some()
    }

    pub fn goal_label_at(&self, cell: u16) -> Option<Label> {
        self.goal_at[cell as usize].map(Label)
    }

    pub fn goal_matches(&self, cell: u16, label_group: u8) -> bool {
        self.goal_at[cell as usize] == Some(label_group)
    }

    pub fn reverse_push_distance(&self, goal_index: usize, cell: u16) -> u16 {
        self.reverse_push_dist[goal_index * self.cell_count as usize + cell as usize]
    }

    fn compute_reverse_push_distances(
        cell_count: u16,
        neighbors: &[[u16; 4]],
        goal_cells: &[(u16, Label)],
    ) -> Vec<u16> {
        let mut dist = Vec::with_capacity(goal_cells.len() * cell_count as usize);
        for &(goal_cell, _) in goal_cells {
            dist.extend_from_slice(&Self::bfs_reverse_push(cell_count, neighbors, goal_cell));
        }
        dist
    }

    /// BFS from goal_cell computing minimum push distance to reach it.
    /// Reverse-push: from goal, pull in each direction. A box at cell C can be
    /// pulled in direction D if neighbor(C, D) exists (box came from there)
    /// and neighbor(C, opposite(D)) exists (keeper stood there to push).
    fn bfs_reverse_push(cell_count: u16, neighbors: &[[u16; 4]], goal_cell: u16) -> Vec<u16> {
        let mut dist = vec![u16::MAX; cell_count as usize];
        let mut queue = VecDeque::new();

        dist[goal_cell as usize] = 0;
        queue.push_back(goal_cell);

        while let Some(cell) = queue.pop_front() {
            let d = dist[cell as usize];

            for dir in Direction::ALL {
                let box_from = neighbors[cell as usize][dir as usize];
                if box_from == INVALID_CELL {
                    continue;
                }
                let keeper_at = neighbors[box_from as usize][dir as usize];
                if keeper_at == INVALID_CELL {
                    continue;
                }
                if dist[box_from as usize] > d + 1 {
                    dist[box_from as usize] = d + 1;
                    queue.push_back(box_from);
                }
            }
        }

        dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    fn test_board() -> ParsedBoard {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        parse_board(rows).unwrap()
    }

    #[test]
    fn cell_count_matches_floor() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        assert_eq!(cb.cell_count as usize, board.floor.len());
    }

    #[test]
    fn position_roundtrip() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        for cell in 0..cb.cell_count {
            let pos = cb.cell_to_pos(cell);
            assert_eq!(cb.pos_to_cell(pos), cell);
        }
    }

    #[test]
    fn neighbor_consistency() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        for cell in 0..cb.cell_count {
            for dir in Direction::ALL {
                let n = cb.neighbor(cell, dir);
                if n != INVALID_CELL {
                    let back = cb.neighbor(n, dir.opposite());
                    assert_eq!(back, cell);
                }
            }
        }
    }

    #[test]
    fn walls_excluded() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        for &w in &board.walls {
            assert_eq!(cb.pos_to_cell(w), INVALID_CELL);
        }
    }

    #[test]
    fn goal_cell_found() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        assert_eq!(cb.goal_cells.len(), 1);
        let goal_pos = cb.cell_to_pos(cb.goal_cells[0].0);
        assert_eq!(goal_pos, Position::new(1, 3));
    }

    #[test]
    fn reverse_push_distance_zero_at_goal() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        let goal_cell = cb.goal_cells[0].0;
        assert_eq!(cb.reverse_push_distance(0, goal_cell), 0);
    }

    #[test]
    fn reverse_push_distance_reachable() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        let box_cell = cb.pos_to_cell(Position::new(2, 2));
        let dist = cb.reverse_push_distance(0, box_cell);
        assert!(dist < u16::MAX, "box should be reachable from goal");
        assert!(dist > 0, "box is not on goal");
    }

    #[test]
    fn out_of_bounds_is_invalid() {
        let board = test_board();
        let cb = CompiledBoard::from_parsed(&board);
        assert_eq!(cb.pos_to_cell(Position::new(-1, 0)), INVALID_CELL);
        assert_eq!(cb.pos_to_cell(Position::new(0, 100)), INVALID_CELL);
    }
}
