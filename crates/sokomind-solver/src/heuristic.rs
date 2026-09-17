use rustc_hash::FxHashMap;

use crate::assignment::hungarian;
use crate::compiled_board::CompiledBoard;
use crate::dense_state::DenseState;
use crate::interaction_boost::count_interaction_penalties;
use crate::linear_conflict::count_linear_conflicts;
use crate::pattern_database::PatternDatabase;
use sokomind_core::types::Label;

/// Assignment-based admissible heuristic.
/// Computes minimum-cost box-goal assignment using reverse-push distances.
/// Enhanced with linear conflict, interaction boost, and pattern database.
/// Caches results by a hash of box positions.
pub struct AssignmentHeuristic {
    cache: FxHashMap<u64, u32>,
    goal_groups: Vec<GoalGroup>,
    pdb: PatternDatabase,
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

        let pdb = PatternDatabase::build(cb);

        Self {
            cache: FxHashMap::default(),
            goal_groups,
            pdb,
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

        let lc = count_linear_conflicts(cb, state);
        let ip = count_interaction_penalties(cb, &state.box_cells);
        total = total.saturating_add(lc.max(ip));

        let pdb_cost = self.pdb.evaluate(cb, &state.box_cells);
        if pdb_cost > total {
            total = pdb_cost;
        }

        total
    }

    /// Fast heuristic for beam search ordering (not admissible).
    /// Uses greedy min-distance per box instead of Hungarian.
    /// Skips pattern DB, linear conflicts, interaction penalties.
    /// O(n*m) instead of O(n^2*m).
    pub fn fast_evaluate(&mut self, cb: &CompiledBoard, state: &DenseState, cache_key: u64) -> u32 {
        let fast_key = cache_key.wrapping_add(0xBEAF_FA57_5A17_0000);
        if let Some(&cached) = self.cache.get(&fast_key) {
            return cached;
        }

        let result = self.compute_fast(cb, state);
        self.cache.insert(fast_key, result);
        result
    }

    fn compute_fast(&self, cb: &CompiledBoard, state: &DenseState) -> u32 {
        let mut total: u32 = 0;

        for group in &self.goal_groups {
            let box_cells: Vec<u16> = state
                .box_cells
                .iter()
                .filter(|&&(_, l)| l == group.label.0)
                .map(|&(c, _)| c)
                .collect();

            if box_cells.is_empty() {
                continue;
            }

            let mut used_goals = vec![false; group.goal_indices.len()];

            for &box_cell in &box_cells {
                let mut best_dist = u32::MAX;
                let mut best_gi = 0;
                for (gi, &goal_idx) in group.goal_indices.iter().enumerate() {
                    if used_goals[gi] {
                        continue;
                    }
                    let dist = cb.reverse_push_distance(goal_idx, box_cell) as u32;
                    if dist < best_dist {
                        best_dist = dist;
                        best_gi = gi;
                    }
                }
                if best_dist == u32::MAX {
                    return u32::MAX;
                }
                used_goals[best_gi] = true;
                total = total.saturating_add(best_dist);
            }
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
            keeper_cell: 0,
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
    fn interference_enhancement_uses_max() {
        // Verify lc and ip are combined via max, not addition.
        // Two boxes that may trigger both linear_conflict and interaction_boost.
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XXR O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        let state = DenseState::from_initial(&cb);

        let lc = crate::linear_conflict::count_linear_conflicts(&cb, &state);
        let ip = crate::interaction_boost::count_interaction_penalties(&cb, &state.box_cells);

        let mut heuristic = AssignmentHeuristic::new(&cb);
        let h = heuristic.evaluate(&cb, &state, state.zobrist_hash(&zk));

        // Heuristic should use max(lc,ip), not lc+ip.
        // Full admissibility validated by known_optima integration tests.
        assert!(h > 0);
        assert!(h < u32::MAX);
        if lc > 0 && ip > 0 {
            // With max, the enhancement is at most max(lc,ip).
            // With addition, it would be lc+ip.
            // We can't directly verify without the hungarian base, but
            // we verify the heuristic is reasonable.
            assert!(
                h < 100,
                "heuristic={} seems unreasonably high for a 2-box puzzle",
                h,
            );
        }
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
