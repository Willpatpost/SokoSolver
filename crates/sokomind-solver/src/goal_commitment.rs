use crate::assignment::hungarian;
use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use sokomind_core::position::Direction;
use sokomind_core::types::Label;

pub struct GoalCommitmentDetector {
    has_potential: bool,
}

impl GoalCommitmentDetector {
    pub fn new(cb: &CompiledBoard) -> Self {
        let has_potential = cb.goal_cells.iter().any(|&(cell, _)| is_corner(cb, cell));
        GoalCommitmentDetector { has_potential }
    }

    pub fn has_potential(&self) -> bool {
        self.has_potential
    }

    pub fn find_committed_boxes(&self, cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u64 {
        if !self.has_potential {
            return 0;
        }

        let mut mask = 0u64;

        for (i, &(cell, label)) in box_cells.iter().enumerate() {
            if i >= 64 {
                break;
            }
            if !cb.goal_matches(cell, label) {
                continue;
            }
            if !is_corner(cb, cell) {
                continue;
            }
            if !residual_assignment_feasible(cb, box_cells, i) {
                continue;
            }
            mask |= 1u64 << i;
        }

        mask
    }

    /// Aggressive commitment: any box on its matching goal is committed if
    /// the remaining boxes can still reach their goals (no corner requirement).
    /// Suitable for endgame search where most boxes are placed.
    pub fn find_committed_boxes_aggressive(&self, cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u64 {
        let mut mask = 0u64;

        for (i, &(cell, label)) in box_cells.iter().enumerate() {
            if i >= 64 {
                break;
            }
            if !cb.goal_matches(cell, label) {
                continue;
            }
            mask |= 1u64 << i;
        }

        if mask == 0 {
            return 0;
        }

        // Verify each candidate: removing it must leave a feasible assignment
        let mut verified = 0u64;
        for i in 0..box_cells.len().min(64) {
            if (mask & (1u64 << i)) == 0 {
                continue;
            }
            if residual_assignment_feasible(cb, box_cells, i) {
                verified |= 1u64 << i;
            }
        }

        verified
    }
}

fn is_corner(cb: &CompiledBoard, cell: u16) -> bool {
    let up = cb.neighbor(cell, Direction::Up) == INVALID_CELL;
    let down = cb.neighbor(cell, Direction::Down) == INVALID_CELL;
    let left = cb.neighbor(cell, Direction::Left) == INVALID_CELL;
    let right = cb.neighbor(cell, Direction::Right) == INVALID_CELL;
    (up && left) || (up && right) || (down && left) || (down && right)
}

fn residual_assignment_feasible(
    cb: &CompiledBoard,
    box_cells: &[(u16, u8)],
    exclude_index: usize,
) -> bool {
    let excluded_cell = box_cells[exclude_index].0;
    let excluded_label = box_cells[exclude_index].1;

    let remaining_boxes: Vec<u16> = box_cells
        .iter()
        .enumerate()
        .filter(|&(i, &(_, l))| i != exclude_index && l == excluded_label)
        .map(|(_, &(c, _))| c)
        .collect();

    let mut residual_goal_indices: Vec<usize> = Vec::new();
    for (gi, &(gcell, glabel)) in cb.goal_cells.iter().enumerate() {
        if gcell == excluded_cell && glabel == Label(excluded_label) {
            continue;
        }
        if glabel == Label(excluded_label) {
            residual_goal_indices.push(gi);
        }
    }

    if remaining_boxes.len() != residual_goal_indices.len() {
        return false;
    }
    if remaining_boxes.is_empty() {
        return true;
    }

    let cap = 10_000u32;
    let cost_matrix: Vec<Vec<u32>> = remaining_boxes
        .iter()
        .map(|&box_cell| {
            residual_goal_indices
                .iter()
                .map(|&gi| {
                    let d = cb.reverse_push_distance(gi, box_cell) as u32;
                    if d == u16::MAX as u32 || d > cap {
                        cap
                    } else {
                        d
                    }
                })
                .collect()
        })
        .collect();

    if cost_matrix
        .iter()
        .any(|row| row.iter().all(|&d| d >= cap))
    {
        return false;
    }

    let (total, _) = hungarian(&cost_matrix);
    total < cap * remaining_boxes.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dense_state::DenseState;
    use sokomind_core::board::parse_board;

    #[test]
    fn no_commitment_when_not_on_goal() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let state = DenseState::from_initial(&cb);
        let detector = GoalCommitmentDetector::new(&cb);

        let mask = detector.find_committed_boxes(&cb, &state.box_cells);
        assert_eq!(mask, 0);
    }

    #[test]
    fn commitment_on_corner_goal() {
        let rows = &["OOOOO", "OS  O", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let detector = GoalCommitmentDetector::new(&cb);

        let goal_cell = cb.goal_cells[0].0;
        let goal_label = cb.goal_cells[0].1 .0;
        let box_cells = vec![(goal_cell, goal_label)];

        let mask = detector.find_committed_boxes(&cb, &box_cells);
        assert_ne!(mask, 0);
    }

    #[test]
    fn no_commitment_without_residual_feasibility() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O XXR O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let detector = GoalCommitmentDetector::new(&cb);

        let state = DenseState::from_initial(&cb);
        let mask = detector.find_committed_boxes(&cb, &state.box_cells);
        assert_eq!(mask, 0, "boxes not on goals should not commit");
    }

    #[test]
    fn has_potential_detects_corner_goals() {
        let rows = &["OOOOO", "OS  O", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let detector = GoalCommitmentDetector::new(&cb);
        assert!(detector.has_potential());
    }

    #[test]
    fn no_potential_without_corner_goals() {
        let rows = &[
            "OOOOOOO", "O     O", "O S   O", "O XR  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let detector = GoalCommitmentDetector::new(&cb);
        assert!(!detector.has_potential());
    }
}
