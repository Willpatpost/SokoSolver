use rustc_hash::FxHashMap;
use std::collections::VecDeque;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use sokomind_core::position::Direction;

const MAX_PATTERN_SIZE: usize = 4;
const MAX_PDB_STATES: usize = 500_000;

/// Additive pattern database for admissible heuristic enhancement.
///
/// Partitions goals into small groups (up to MAX_PATTERN_SIZE each).
/// For each group, runs reverse BFS from the goal configuration to
/// enumerate reachable states and their optimal push distances.
/// At query time, sums the cost for each pattern group — this is
/// admissible because the groups are disjoint (each goal in exactly
/// one group).
pub struct PatternDatabase {
    patterns: Vec<Pattern>,
}

struct Pattern {
    goal_indices: Vec<usize>,
    label: u8,
    table: FxHashMap<Vec<u16>, u32>,
}

impl PatternDatabase {
    /// Build pattern databases for the given board.
    /// Groups goals by label, then partitions each label's goals into
    /// chunks of at most MAX_PATTERN_SIZE.
    pub fn build(cb: &CompiledBoard) -> Self {
        let mut label_goals: FxHashMap<u8, Vec<usize>> = FxHashMap::default();
        for (gi, &(_, label)) in cb.goal_cells.iter().enumerate() {
            label_goals.entry(label.0).or_default().push(gi);
        }

        let mut patterns = Vec::new();

        for (&label, goal_indices) in &label_goals {
            for chunk in goal_indices.chunks(MAX_PATTERN_SIZE) {
                if chunk.len() < 2 {
                    continue;
                }
                if let Some(table) = build_pattern_table(cb, chunk) {
                    patterns.push(Pattern {
                        goal_indices: chunk.to_vec(),
                        label,
                        table,
                    });
                }
            }
        }

        PatternDatabase { patterns }
    }

    /// Query the pattern database for a lower bound on pushes needed.
    /// Sums costs across all pattern groups. Returns 0 if no patterns
    /// were built (too few goals or board too large).
    pub fn evaluate(&self, cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> u32 {
        let mut total = 0u32;

        for pattern in &self.patterns {
            let relevant_boxes: Vec<u16> = box_cells
                .iter()
                .filter(|&&(_, l)| l == pattern.label)
                .map(|&(c, _)| c)
                .collect();

            if relevant_boxes.len() < pattern.goal_indices.len() {
                continue;
            }

            if let Some(cost) = lookup_best_match(cb, pattern, &relevant_boxes) {
                total = total.saturating_add(cost);
            }
        }

        total
    }

    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    pub fn total_entries(&self) -> usize {
        self.patterns.iter().map(|p| p.table.len()).sum()
    }
}

fn build_pattern_table(
    cb: &CompiledBoard,
    goal_indices: &[usize],
) -> Option<FxHashMap<Vec<u16>, u32>> {
    let goal_cells: Vec<u16> = goal_indices.iter().map(|&gi| cb.goal_cells[gi].0).collect();

    let mut table: FxHashMap<Vec<u16>, u32> = FxHashMap::default();
    let mut queue: VecDeque<(Vec<u16>, u32)> = VecDeque::new();

    let mut initial = goal_cells.clone();
    initial.sort();
    table.insert(initial.clone(), 0);
    queue.push_back((initial, 0));

    while let Some((positions, dist)) = queue.pop_front() {
        if table.len() > MAX_PDB_STATES {
            break;
        }

        for (pi, &pos) in positions.iter().enumerate() {
            for dir in Direction::ALL {
                let box_from = cb.neighbor(pos, dir);
                if box_from == INVALID_CELL {
                    continue;
                }
                let keeper_at = cb.neighbor(box_from, dir);
                if keeper_at == INVALID_CELL {
                    continue;
                }

                if positions.contains(&box_from) {
                    continue;
                }

                let mut new_positions = positions.clone();
                new_positions[pi] = box_from;
                new_positions.sort();

                let new_dist = dist + 1;
                if let Some(&existing) = table.get(&new_positions) {
                    if existing <= new_dist {
                        continue;
                    }
                }

                table.insert(new_positions.clone(), new_dist);
                queue.push_back((new_positions, new_dist));
            }
        }
    }

    if table.len() > 1 {
        Some(table)
    } else {
        None
    }
}

fn lookup_best_match(_cb: &CompiledBoard, pattern: &Pattern, box_positions: &[u16]) -> Option<u32> {
    let n_pattern = pattern.goal_indices.len();
    if box_positions.len() < n_pattern {
        return None;
    }

    if box_positions.len() == n_pattern {
        let mut key: Vec<u16> = box_positions.to_vec();
        key.sort();
        return pattern.table.get(&key).copied();
    }

    let mut best: Option<u32> = None;
    choose_combinations(box_positions, n_pattern, &mut Vec::new(), &mut |combo| {
        let mut key: Vec<u16> = combo.to_vec();
        key.sort();
        if let Some(&cost) = pattern.table.get(&key) {
            best = Some(best.map_or(cost, |b: u32| b.min(cost)));
        }
    });

    best
}

fn choose_combinations<F>(items: &[u16], k: usize, current: &mut Vec<u16>, callback: &mut F)
where
    F: FnMut(&[u16]),
{
    if current.len() == k {
        callback(current);
        return;
    }

    let remaining = k - current.len();
    let start = if current.is_empty() {
        0
    } else {
        items
            .iter()
            .position(|&x| x == *current.last().unwrap())
            .unwrap_or(0)
            + 1
    };

    for i in start..items.len() {
        if items.len() - i < remaining {
            break;
        }
        current.push(items[i]);
        choose_combinations(items, k, current, callback);
        current.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn single_goal_no_patterns() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let pdb = PatternDatabase::build(&cb);
        assert_eq!(pdb.pattern_count(), 0);
    }

    #[test]
    fn two_goals_builds_pattern() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let pdb = PatternDatabase::build(&cb);
        assert!(pdb.pattern_count() >= 1);
        assert!(pdb.total_entries() > 1);
    }

    #[test]
    fn evaluate_at_goal_is_zero() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let pdb = PatternDatabase::build(&cb);

        let box_cells: Vec<(u16, u8)> = cb.goal_cells.iter().map(|&(c, l)| (c, l.0)).collect();
        let h = pdb.evaluate(&cb, &box_cells);
        assert_eq!(h, 0);
    }

    #[test]
    fn evaluate_off_goal_positive() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let pdb = PatternDatabase::build(&cb);

        let box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        let h = pdb.evaluate(&cb, &box_cells);
        assert!(h > 0, "boxes off-goal should have positive heuristic");
    }

    #[test]
    fn admissibility_check() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let pdb = PatternDatabase::build(&cb);

        let box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        let h = pdb.evaluate(&cb, &box_cells);
        // PDB should never exceed the actual optimal cost. On this
        // 2-box puzzle, optimal is ~4 pushes.
        assert!(
            h <= 10,
            "PDB should give a reasonable lower bound, got {}",
            h
        );
    }

    #[test]
    fn typed_labels_separate_patterns() {
        let rows = &[
            "OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let pdb = PatternDatabase::build(&cb);
        // Each label has only 1 goal → no patterns (need ≥2).
        assert_eq!(pdb.pattern_count(), 0);
    }

    #[test]
    fn choose_combinations_basic() {
        let items = vec![1u16, 2, 3, 4];
        let mut results = Vec::new();
        choose_combinations(&items, 2, &mut Vec::new(), &mut |combo| {
            results.push(combo.to_vec());
        });
        assert_eq!(results.len(), 6); // C(4,2) = 6
    }
}
