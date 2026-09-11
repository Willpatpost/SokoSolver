use sokomind_core::position::Direction;

use crate::compiled_board::CompiledBoard;
use crate::reachability::find_keeper_path;

/// Re-optimize the walk (non-push) segments in a full step sequence.
///
/// Between each pair of consecutive pushes, the keeper walks from one
/// push-from position to the next. This pass replaces each walk segment
/// with the BFS-shortest path given the current box configuration at that
/// point.
///
/// Returns the total moves saved (>= 0).
pub fn optimize_walks(cb: &CompiledBoard, steps: &mut Vec<(Direction, bool)>) -> u32 {
    let initial_len = steps.len() as u32;
    let mut optimized: Vec<(Direction, bool)> = Vec::with_capacity(steps.len());

    let mut box_cells: Vec<(u16, u8)> = cb
        .initial_box_cells
        .iter()
        .map(|&(c, l)| (c, l.0))
        .collect();
    box_cells.sort();

    let mut keeper = cb.robot_cell;

    let mut i = 0;
    while i < steps.len() {
        if steps[i].1 {
            // Push step: emit it directly and update state.
            let dir = steps[i].0;
            let box_cell = cb.neighbor(keeper, dir);
            let target = cb.neighbor(box_cell, dir);

            // Update box position.
            if let Ok(idx) = box_cells.binary_search_by_key(&box_cell, |&(c, _)| c) {
                let label = box_cells[idx].1;
                box_cells[idx] = (target, label);
                box_cells.sort();
            }

            optimized.push((dir, true));
            keeper = box_cell;
            i += 1;
        } else {
            // Walk segment: collect all consecutive walk steps.
            let walk_start = keeper;
            let mut walk_end = keeper;
            let seg_start = i;
            while i < steps.len() && !steps[i].1 {
                walk_end = cb.neighbor(walk_end, steps[i].0);
                i += 1;
            }

            // Try to find a shorter path via BFS.
            match find_keeper_path(cb, walk_start, walk_end, &box_cells) {
                Some(path) if path.len() < (i - seg_start) => {
                    for &d in &path {
                        optimized.push((d, false));
                    }
                }
                _ => {
                    for step in &steps[seg_start..i] {
                        optimized.push(*step);
                    }
                }
            }

            keeper = walk_end;
        }
    }

    let saved = initial_len.saturating_sub(optimized.len() as u32);
    *steps = optimized;
    saved
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn no_walks_no_change() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Single push, no walks.
        let mut steps = vec![(Direction::Left, true)];
        let saved = optimize_walks(&cb, &mut steps);
        assert_eq!(saved, 0);
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn already_optimal_walks_unchanged() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        // Walk right (already minimal 1-step), then push.
        let mut steps = vec![(Direction::Down, false), (Direction::Left, true)];
        let original_len = steps.len();
        let saved = optimize_walks(&cb, &mut steps);
        assert_eq!(saved, 0);
        assert_eq!(steps.len(), original_len);
    }

    #[test]
    fn redundant_walk_shortened() {
        // Open room. Robot at (4,3). Box at (1,1), goal at (1,5) — far from walk.
        // Detour from (4,3) to (4,1) via (3,3)->(3,2)->(3,1)->(4,1) = 4 steps.
        // Direct: (4,3)->(4,2)->(4,1) = 2 steps (no obstacle).
        let rows = &[
            "OOOOOOO", "OX   SO", "O     O", "O     O", "O  R  O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);

        let mut steps = vec![
            (Direction::Up, false),
            (Direction::Left, false),
            (Direction::Left, false),
            (Direction::Down, false),
        ];
        let saved = optimize_walks(&cb, &mut steps);
        assert!(saved >= 2, "should save at least 2 steps, saved {}", saved);
        assert_eq!(steps.len(), 2);
    }
}
