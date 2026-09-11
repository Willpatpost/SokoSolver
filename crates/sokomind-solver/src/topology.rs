use sokomind_core::position::Direction;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};

/// Board topology analysis: identifies dead cells, simple deadlocks.
#[derive(Clone, Debug)]
pub struct BoardTopology {
    pub dead_cells: Vec<bool>,
    pub corner_cells: Vec<bool>,
}

impl BoardTopology {
    pub fn analyze(cb: &CompiledBoard) -> Self {
        let n = cb.cell_count as usize;
        let mut corner_cells = vec![false; n];

        for cell in 0..cb.cell_count {
            if cb.is_goal(cell) {
                continue;
            }
            corner_cells[cell as usize] = is_corner(cb, cell);
        }

        let dead_cells = compute_dead_cells(cb);

        BoardTopology {
            dead_cells,
            corner_cells,
        }
    }

    pub fn is_dead(&self, cell: u16) -> bool {
        self.dead_cells[cell as usize]
    }

    pub fn is_corner(&self, cell: u16) -> bool {
        self.corner_cells[cell as usize]
    }
}

fn is_corner(cb: &CompiledBoard, cell: u16) -> bool {
    let up = cb.neighbor(cell, Direction::Up) == INVALID_CELL;
    let down = cb.neighbor(cell, Direction::Down) == INVALID_CELL;
    let left = cb.neighbor(cell, Direction::Left) == INVALID_CELL;
    let right = cb.neighbor(cell, Direction::Right) == INVALID_CELL;

    (up && left) || (up && right) || (down && left) || (down && right)
}

/// A cell is dead if no box placed there can ever reach any goal.
/// We mark non-goal corners as dead, plus cells on wall-hugging lines
/// between two dead corners with no goal in between.
fn compute_dead_cells(cb: &CompiledBoard) -> Vec<bool> {
    let n = cb.cell_count as usize;
    let mut dead = vec![false; n];

    for cell in 0..cb.cell_count {
        if cb.is_goal(cell) {
            continue;
        }
        if is_corner(cb, cell) {
            dead[cell as usize] = true;
        }
    }

    // Wall-hugging lines between dead corners
    for cell in 0..cb.cell_count {
        if !dead[cell as usize] {
            continue;
        }
        for &(walk_dir, wall_dir) in &[
            (Direction::Right, Direction::Up),
            (Direction::Right, Direction::Down),
            (Direction::Down, Direction::Left),
            (Direction::Down, Direction::Right),
        ] {
            mark_dead_line(cb, &mut dead, cell, walk_dir, wall_dir);
        }
    }

    dead
}

fn mark_dead_line(
    cb: &CompiledBoard,
    dead: &mut [bool],
    start: u16,
    walk_dir: Direction,
    wall_dir: Direction,
) {
    let mut candidates = Vec::new();
    let mut cell = start;

    loop {
        let next = cb.neighbor(cell, walk_dir);
        if next == INVALID_CELL {
            break;
        }
        cell = next;

        if cb.neighbor(cell, wall_dir) != INVALID_CELL {
            return;
        }

        if cb.is_goal(cell) {
            return;
        }

        candidates.push(cell);

        if dead[cell as usize] {
            break;
        }
    }

    if !dead[cell as usize] {
        return;
    }

    for &c in &candidates {
        dead[c as usize] = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn corner_detection() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let corner = cb.pos_to_cell(Position::new(1, 1));
        assert!(topo.is_corner(corner), "(1,1) is a corner");

        let non_corner = cb.pos_to_cell(Position::new(2, 2));
        assert!(!topo.is_corner(non_corner), "(2,2) is not a corner");
    }

    #[test]
    fn goals_not_dead() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        for &(gc, _) in &cb.goal_cells {
            assert!(!topo.is_dead(gc), "goal cell should not be dead");
        }
    }

    #[test]
    fn non_goal_corners_are_dead() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let corner = cb.pos_to_cell(Position::new(1, 1));
        assert!(topo.is_dead(corner));
    }

    #[test]
    fn wall_line_dead_cells() {
        // OOOOOO
        // O   SO
        // O XR O
        // OOOOOO
        let rows = &["OOOOOO", "O   SO", "O XR O", "OOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        // Top wall: (1,1) and (1,4) are corners (if not goal).
        // (1,3) is the goal S, so the line from (1,1) toward (1,3) should stop.
        // Bottom wall: (2,1) and (2,4) are corners.
        // (2,2) and (2,3) are between them along bottom wall → dead.
        let c = cb.pos_to_cell(Position::new(2, 1));
        assert!(topo.is_dead(c), "(2,1) should be dead");
    }
}
