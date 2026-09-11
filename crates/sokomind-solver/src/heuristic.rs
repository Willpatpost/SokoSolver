use rustc_hash::FxHashMap;

use crate::assignment::hungarian;
use crate::compiled_board::CompiledBoard;
use crate::dense_state::DenseState;
use sokomind_core::types::Label;

/// Assignment-based admissible heuristic.
/// Computes minimum-cost box-goal assignment using reverse-push distances.
/// Caches results by a hash of box positions.
pub struct AssignmentHeuristic {
    cache: FxHashMap<u64, u32>,
    goal_groups: Vec<GoalGroup>,
}

struct GoalGroup {
    label: Label,
    goal_indices: Vec<usize>,
}

impl AssignmentHeuristic {
    pub fn new(cb: &CompiledBoard) -> Self {
        let mut label_to_goals: FxHashMap<u8, Vec<usize>> = FxHashMap::default();
        for (i, &(_, label)) in cb.goal_cells.iter().enumerate() {
            label_to_goals.entry(label.0).or_default().push(i);
        }

        let goal_groups: Vec<GoalGroup> = label_to_goals
            .into_iter()
            .map(|(label_id, goal_indices)| GoalGroup {
                label: Label(label_id),
                goal_indices,
            })
            .collect();

        Self {
            cache: FxHashMap::default(),
            goal_groups,
        }
    }

    /// Compute lower bound on pushes needed.
    /// Returns u32::MAX if any box cannot reach any compatible goal.
    pub fn evaluate(&mut self, cb: &CompiledBoard, state: &DenseState, cache_key: u64) -> u32 {
        if let Some(&cached) = self.cache.get(&cache_key) {
            return cached;
        }

        let result = self.compute(cb, state);
        self.cache.insert(cache_key, result);
        result
    }

    fn compute(&self, cb: &CompiledBoard, state: &DenseState) -> u32 {
        let mut total: u32 = 0;

        for group in &self.goal_groups {
            let box_cells: Vec<(u16, u8)> = state
                .box_cells
                .iter()
                .filter(|&&(_, l)| l == group.label.0)
                .copied()
                .collect();

            if box_cells.is_empty() {
                continue;
            }

            let n_boxes = box_cells.len();
            let n_goals = group.goal_indices.len();

            let mut cost_matrix = vec![vec![u32::MAX; n_goals]; n_boxes];

            for (bi, &(box_cell, _)) in box_cells.iter().enumerate() {
                for (gi, &goal_idx) in group.goal_indices.iter().enumerate() {
                    let dist = cb.reverse_push_distance(goal_idx, box_cell);
                    cost_matrix[bi][gi] = dist as u32;
                }
            }

            if cost_matrix
                .iter()
                .any(|row| row.iter().all(|&d| d == u32::MAX))
            {
                return u32::MAX;
            }

            let cap = 10_000u32;
            for row in &mut cost_matrix {
                for val in row.iter_mut() {
                    if *val == u32::MAX || *val > cap {
                        *val = cap;
                    }
                }
            }

            let (group_cost, _) = hungarian(&cost_matrix);
            total = total.saturating_add(group_cost);
        }

        total
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zobrist::ZobristKeys;
    use sokomind_core::board::parse_board;

    #[test]
    fn solved_state_has_zero_heuristic() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));

        let goal_cell = cb.goal_cells[0].0;
        let goal_label = cb.goal_cells[0].1;

        let state = DenseState {
            keeper_zone: 0,
            box_cells: vec![(goal_cell, goal_label.0)],
            moves: 0,
            pushes: 0,
        };

        let mut heuristic = AssignmentHeuristic::new(&cb);
        let h = heuristic.evaluate(&cb, &state, state.zobrist_hash(&zk));
        assert_eq!(h, 0);
    }

    #[test]
    fn initial_state_positive_heuristic() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));

        let state = DenseState::from_initial(&cb);
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let h = heuristic.evaluate(&cb, &state, state.zobrist_hash(&zk));
        assert!(h > 0, "box is not on goal, heuristic should be positive");
    }

    #[test]
    fn heuristic_is_admissible() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));

        let state = DenseState::from_initial(&cb);
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let h = heuristic.evaluate(&cb, &state, state.zobrist_hash(&zk));
        // The box at (2,2) needs to reach goal at (1,3) — at least 2 pushes (up, right)
        assert!(h <= 3, "heuristic should be a lower bound");
    }

    #[test]
    fn cache_hit() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));

        let state = DenseState::from_initial(&cb);
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let key = state.zobrist_hash(&zk);
        let h1 = heuristic.evaluate(&cb, &state, key);
        assert_eq!(heuristic.cache_len(), 1);
        let h2 = heuristic.evaluate(&cb, &state, key);
        assert_eq!(h1, h2);
    }

    #[test]
    fn typed_labels() {
        let rows = &["OOOOOOO", "O     O", "ORAa  O", "OB  b O", "OOOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));

        let state = DenseState::from_initial(&cb);
        let mut heuristic = AssignmentHeuristic::new(&cb);
        let h = heuristic.evaluate(&cb, &state, state.zobrist_hash(&zk));
        assert!(h > 0);
    }
}
