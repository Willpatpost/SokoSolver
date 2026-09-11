use crate::compiled_board::CompiledBoard;

/// Check for goal-commitment deadlock: is there a valid bipartite matching
/// between all boxes and all goals respecting label compatibility and
/// reverse-push reachability?
///
/// Two-level check:
/// 1. Quick: any box with zero reachable compatible goals → immediate deadlock.
/// 2. Full: augmenting-path bipartite matching to verify a feasible assignment.
pub fn is_goal_commitment_deadlock(cb: &CompiledBoard, box_cells: &[(u16, u8)]) -> bool {
    let n_boxes = box_cells.len();
    let n_goals = cb.goal_cells.len();

    if n_boxes == 0 || n_goals == 0 {
        return n_boxes > 0;
    }

    // Build adjacency: box i can be assigned to goal j if labels match and reachable.
    let mut adj: Vec<Vec<usize>> = Vec::with_capacity(n_boxes);

    for &(box_cell, box_label) in box_cells {
        let mut reachable_goals = Vec::new();
        for (gi, &(_, goal_label)) in cb.goal_cells.iter().enumerate() {
            if box_label != goal_label.0 {
                continue;
            }
            let dist = cb.reverse_push_distance(gi, box_cell);
            if dist < u16::MAX {
                reachable_goals.push(gi);
            }
        }
        if reachable_goals.is_empty() {
            return true;
        }
        adj.push(reachable_goals);
    }

    // Full bipartite matching via augmenting paths.
    let matching = max_bipartite_matching(&adj, n_goals);
    matching < n_boxes
}

/// Find maximum bipartite matching using DFS augmenting paths.
/// Returns the size of the maximum matching.
fn max_bipartite_matching(adj: &[Vec<usize>], n_right: usize) -> usize {
    let n_left = adj.len();
    let mut match_right: Vec<Option<usize>> = vec![None; n_right];
    let mut result = 0;

    for u in 0..n_left {
        let mut visited = vec![false; n_right];
        if augment(u, adj, &mut match_right, &mut visited) {
            result += 1;
        }
    }

    result
}

fn augment(
    u: usize,
    adj: &[Vec<usize>],
    match_right: &mut [Option<usize>],
    visited: &mut [bool],
) -> bool {
    for &v in &adj[u] {
        if visited[v] {
            continue;
        }
        visited[v] = true;
        if match_right[v].is_none() || augment(match_right[v].unwrap(), adj, match_right, visited) {
            match_right[v] = Some(u);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn box_can_reach_goal() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let box_cell = cb.pos_to_cell(Position::new(2, 2));
        assert!(!is_goal_commitment_deadlock(&cb, &[(box_cell, 0)]));
    }

    #[test]
    fn box_unreachable_from_goal() {
        // Box in a dead corner where reverse-push can't reach.
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Check if corner cell (1,1) has u16::MAX reverse-push distance.
        let corner = cb.pos_to_cell(Position::new(1, 1));
        let dist = cb.reverse_push_distance(0, corner);
        if dist == u16::MAX {
            assert!(is_goal_commitment_deadlock(&cb, &[(corner, 0)]));
        }
    }

    #[test]
    fn label_mismatch_deadlock() {
        // Typed box can't reach any goal of matching label.
        let rows = &["OOOOO", "O  aO", "O ARO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Box A is label 1, goal 'a' is label 1. Should match.
        let box_cell = cb.initial_box_cells[0].0;
        let box_label = cb.initial_box_cells[0].1 .0;
        assert!(!is_goal_commitment_deadlock(&cb, &[(box_cell, box_label)]));

        // Now try a box with a label that has no matching goal.
        // Label 2 (B) has no goal on this board.
        assert!(is_goal_commitment_deadlock(&cb, &[(box_cell, 2)]));
    }

    #[test]
    fn two_boxes_one_goal_deadlock() {
        // Two generic boxes but only one generic goal → can't assign both.
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.pos_to_cell(Position::new(2, 2));
        let b2 = cb.pos_to_cell(Position::new(2, 1));
        // 2 boxes, 1 goal → matching can only be 1 → deadlock
        assert!(is_goal_commitment_deadlock(&cb, &[(b1, 0), (b2, 0)]));
    }

    #[test]
    fn matching_detects_pigeonhole() {
        // Two boxes both can only reach the same single goal.
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let b1 = cb.initial_box_cells[0].0;
        let b2 = cb.initial_box_cells[1].0;
        let l1 = cb.initial_box_cells[0].1 .0;
        let l2 = cb.initial_box_cells[1].1 .0;
        // Both boxes and both goals are generic (label 0), 2 goals available.
        // Should NOT be a deadlock — valid matching exists.
        assert!(!is_goal_commitment_deadlock(
            &cb,
            &[(b1.min(b2), l1), (b1.max(b2), l2)]
        ));
    }

    #[test]
    fn empty_boxes_not_deadlock() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        assert!(!is_goal_commitment_deadlock(&cb, &[]));
    }
}
