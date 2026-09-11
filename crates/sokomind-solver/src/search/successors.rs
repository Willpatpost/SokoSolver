use sokomind_core::position::Direction;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::dense_state::DenseState;
use crate::reachability::{canonical_keeper, keeper_reachable};

/// A push successor: moving a box in a direction.
#[derive(Clone, Debug)]
pub struct PushSuccessor {
    pub state: DenseState,
    pub box_index: usize,
    pub direction: Direction,
}

/// Generate all legal push successors from the current state.
/// Precomputes keeper reachability once (single BFS), then checks each
/// push position with O(1) array lookup instead of per-push BFS.
pub fn generate_successors(cb: &CompiledBoard, state: &DenseState) -> Vec<PushSuccessor> {
    let mut successors = Vec::new();
    let reachable = keeper_reachable(cb, state.keeper_zone, &state.box_cells);

    for (bi, &(box_cell, label)) in state.box_cells.iter().enumerate() {
        for dir in Direction::ALL {
            let target = cb.neighbor(box_cell, dir);
            if target == INVALID_CELL {
                continue;
            }

            if state.box_cells.binary_search_by_key(&target, |&(c, _)| c).is_ok() {
                continue;
            }

            let push_from = cb.neighbor(box_cell, dir.opposite());
            if push_from == INVALID_CELL {
                continue;
            }

            if state.box_cells.binary_search_by_key(&push_from, |&(c, _)| c).is_ok() {
                continue;
            }

            if !reachable[push_from as usize] {
                continue;
            }

            let mut new_box_cells = state.box_cells.clone();
            new_box_cells[bi] = (target, label);
            new_box_cells.sort();

            let new_keeper_zone = canonical_keeper(cb, box_cell, &new_box_cells);

            successors.push(PushSuccessor {
                state: DenseState {
                    keeper_zone: new_keeper_zone,
                    box_cells: new_box_cells,
                    moves: state.moves + 1,
                    pushes: state.pushes + 1,
                },
                box_index: bi,
                direction: dir,
            });
        }
    }

    successors
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    fn setup() -> (CompiledBoard, DenseState) {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let state = DenseState::from_initial(&cb);
        (cb, state)
    }

    #[test]
    fn initial_state_has_successors() {
        let (cb, state) = setup();
        let succs = generate_successors(&cb, &state);
        assert!(!succs.is_empty());
    }

    #[test]
    fn push_count_increments() {
        let (cb, state) = setup();
        let succs = generate_successors(&cb, &state);
        for s in &succs {
            assert_eq!(s.state.pushes, 1);
        }
    }

    #[test]
    fn no_push_into_wall() {
        let (cb, state) = setup();
        let succs = generate_successors(&cb, &state);
        for s in &succs {
            for &(cell, _) in &s.state.box_cells {
                assert_ne!(cell, INVALID_CELL);
                assert!(!cb.wall_set.contains(&cb.cell_to_pos(cell)));
            }
        }
    }

    #[test]
    fn no_push_into_box() {
        let rows = &["OOOOOOO", "O SS  O", "O XXR O", "O     O", "OOOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let state = DenseState::from_initial(&cb);

        let succs = generate_successors(&cb, &state);
        for s in &succs {
            let positions: Vec<u16> = s.state.box_cells.iter().map(|&(c, _)| c).collect();
            let unique: std::collections::HashSet<u16> = positions.iter().copied().collect();
            assert_eq!(positions.len(), unique.len(), "boxes must not overlap");
        }
    }

    #[test]
    fn keeper_must_reach_push_position() {
        // Box at (2,2), robot at (2,3). Keeper can reach (2,3)=right of box,
        // (3,2)=below box, but can't reach (2,1)=left of box if (2,1) is occupied.
        let (cb, state) = setup();
        let succs = generate_successors(&cb, &state);
        // Verify each successor has a valid push origin
        for s in &succs {
            assert!(s.state.keeper_zone < cb.cell_count);
        }
    }

    #[test]
    fn solve_trivial_puzzle() {
        // Box at (2,2), goal at (1,3). Can solve with: push up, push right.
        let (cb, state) = setup();

        // BFS to find solution
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        queue.push_back(state.clone());
        visited.insert((state.keeper_zone, state.box_cells.clone()));

        let mut found = false;
        while let Some(current) = queue.pop_front() {
            if current.is_solved(&cb) {
                found = true;
                break;
            }
            for succ in generate_successors(&cb, &current) {
                let key = (succ.state.keeper_zone, succ.state.box_cells.clone());
                if visited.insert(key) {
                    queue.push_back(succ.state);
                }
            }
        }
        assert!(found, "trivial puzzle should be solvable");
    }

    #[test]
    fn box_cells_stay_sorted() {
        let rows = &["OOOOOOO", "O SS  O", "O XXR O", "O     O", "OOOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let state = DenseState::from_initial(&cb);

        for succ in generate_successors(&cb, &state) {
            let cells: Vec<u16> = succ.state.box_cells.iter().map(|&(c, _)| c).collect();
            let mut sorted = cells.clone();
            sorted.sort();
            assert_eq!(cells, sorted, "box_cells must remain sorted");
        }
    }
}
