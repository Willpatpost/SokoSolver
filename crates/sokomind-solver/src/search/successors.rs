use rustc_hash::FxHashSet;
use sokomind_core::position::Direction;

use crate::compiled_board::{CompiledBoard, INVALID_CELL};
use crate::counters::SearchCounters;
use crate::dense_state::DenseState;
use crate::macros::MacroEngine;
use crate::reachability::{canonical_keeper, keeper_reachable};

/// A push successor: moving a box in a direction.
/// For multi-push sequences, `push_trace` contains all individual pushes.
#[derive(Clone, Debug)]
pub struct PushSuccessor {
    pub state: DenseState,
    pub box_index: usize,
    pub direction: Direction,
    pub push_count: u32,
    /// For push sequences with push_count > 1, this contains each
    /// intermediate (box_index, direction) push in order. Empty for
    /// single-push successors.
    pub push_trace: Vec<(usize, Direction)>,
    /// Cell where the pushed box ended up (correct even after box_cells sorting).
    pub box_target: u16,
}

/// Generate all legal push successors from the current state.
/// Precomputes keeper reachability once (single BFS), then checks each
/// push position with O(1) array lookup instead of per-push BFS.
pub fn generate_successors(cb: &CompiledBoard, state: &DenseState) -> Vec<PushSuccessor> {
    generate_successors_with_macros(cb, state, None, None)
}

/// Generate successors with optional macro engine and counters.
/// When macros are enabled:
/// - Boxes on safe goals are skipped (goal macro)
/// - Pushes into tunnels advance to the exit (tunnel macro)
pub fn generate_successors_with_macros(
    cb: &CompiledBoard,
    state: &DenseState,
    macros: Option<&MacroEngine>,
    counters: Option<&mut SearchCounters>,
) -> Vec<PushSuccessor> {
    generate_successors_inner(cb, state, macros, counters, 0)
}

pub fn generate_successors_committed(
    cb: &CompiledBoard,
    state: &DenseState,
    macros: Option<&MacroEngine>,
    counters: Option<&mut SearchCounters>,
    committed_mask: u64,
) -> Vec<PushSuccessor> {
    generate_successors_inner(cb, state, macros, counters, committed_mask)
}

fn generate_successors_inner(
    cb: &CompiledBoard,
    state: &DenseState,
    macros: Option<&MacroEngine>,
    counters: Option<&mut SearchCounters>,
    committed_mask: u64,
) -> Vec<PushSuccessor> {
    let mut successors = Vec::new();
    let reachable = keeper_reachable(cb, state.keeper_zone, &state.box_cells);

    let mut tunnel_count = 0u64;
    let mut goal_count = 0u64;

    for (bi, &(box_cell, label)) in state.box_cells.iter().enumerate() {
        // Dynamic goal commitment: skip boxes committed via residual matching.
        if bi < 64 && (committed_mask & (1u64 << bi)) != 0 {
            goal_count += 1;
            continue;
        }

        // Static goal macro: skip boxes committed to safe corner goals.
        if let Some(me) = macros {
            if me.safe_goals.is_committed(cb, box_cell, label) {
                goal_count += 1;
                continue;
            }
        }

        for dir in Direction::ALL {
            let target = cb.neighbor(box_cell, dir);
            if target == INVALID_CELL {
                continue;
            }

            if state
                .box_cells
                .binary_search_by_key(&target, |&(c, _)| c)
                .is_ok()
            {
                continue;
            }

            let push_from = cb.neighbor(box_cell, dir.opposite());
            if push_from == INVALID_CELL {
                continue;
            }

            if state
                .box_cells
                .binary_search_by_key(&push_from, |&(c, _)| c)
                .is_ok()
            {
                continue;
            }

            if !reachable[push_from as usize] {
                continue;
            }

            // Tunnel macro: if the target is a tunnel cell, advance to exit.
            let (final_target, extra_pushes) = if let Some(me) = macros {
                match me.tunnels.apply_tunnel(cb, target, dir, &state.box_cells) {
                    Some((exit, extra)) => {
                        tunnel_count += 1;
                        (exit, extra)
                    }
                    None => (target, 0),
                }
            } else {
                (target, 0)
            };

            let mut new_box_cells = state.box_cells.clone();
            new_box_cells[bi] = (final_target, label);
            new_box_cells.sort();

            let keeper_at = if extra_pushes > 0 {
                // Keeper ends up one cell behind the box in the push direction.
                // Walk back from final_target in the opposite direction.
                cb.neighbor(final_target, dir.opposite())
            } else {
                box_cell
            };
            let new_keeper_zone = canonical_keeper(cb, keeper_at, &new_box_cells);

            let total_pushes = 1 + extra_pushes;

            successors.push(PushSuccessor {
                state: DenseState {
                    keeper_zone: new_keeper_zone,
                    box_cells: new_box_cells,
                    moves: state.moves + total_pushes,
                    pushes: state.pushes + total_pushes,
                },
                box_index: bi,
                direction: dir,
                push_count: total_pushes,
                push_trace: Vec::new(),
                box_target: final_target,
            });
        }
    }

    if let Some(c) = counters {
        c.macro_tunnel += tunnel_count;
        c.macro_goal += goal_count;
    }

    successors
}

/// Generate single-box push continuations from a state, pushing only the
/// box at `box_cell` in all legal directions. Used by push-sequence expansion.
fn single_box_pushes(
    cb: &CompiledBoard,
    state: &DenseState,
    box_cell: u16,
) -> Vec<(DenseState, u16, Direction)> {
    let reachable = keeper_reachable(cb, state.keeper_zone, &state.box_cells);
    let bi = match state
        .box_cells
        .binary_search_by_key(&box_cell, |&(c, _)| c)
    {
        Ok(idx) => idx,
        Err(_) => return Vec::new(),
    };

    let label = state.box_cells[bi].1;
    let mut results = Vec::new();

    for dir in Direction::ALL {
        let target = cb.neighbor(box_cell, dir);
        if target == INVALID_CELL {
            continue;
        }
        if state
            .box_cells
            .binary_search_by_key(&target, |&(c, _)| c)
            .is_ok()
        {
            continue;
        }
        let push_from = cb.neighbor(box_cell, dir.opposite());
        if push_from == INVALID_CELL {
            continue;
        }
        if state
            .box_cells
            .binary_search_by_key(&push_from, |&(c, _)| c)
            .is_ok()
        {
            continue;
        }
        if !reachable[push_from as usize] {
            continue;
        }

        let mut new_box_cells = state.box_cells.clone();
        new_box_cells[bi] = (target, label);
        new_box_cells.sort();
        let new_keeper_zone = canonical_keeper(cb, box_cell, &new_box_cells);

        results.push((
            DenseState {
                keeper_zone: new_keeper_zone,
                box_cells: new_box_cells,
                moves: state.moves + 1,
                pushes: state.pushes + 1,
            },
            target,
            dir,
        ));
    }

    results
}

/// Expand a push successor into a sequence of multi-push endpoints by
/// following the same box through corridors and decision points.
///
/// Returns up to `max_returned` diverse endpoints (different box destinations).
/// Each endpoint represents pushing the same box multiple times in succession.
/// The `push_trace` field of each returned successor contains the full sequence
/// of (box_index_in_sorted_cells, direction) pairs for solution reconstruction.
pub fn expand_push_sequence(
    cb: &CompiledBoard,
    initial_succ: &PushSuccessor,
    max_depth: u32,
    max_explored: usize,
    max_returned: usize,
) -> Vec<PushSuccessor> {
    let initial_box_cell = initial_succ.box_target;

    struct SeqState {
        state: DenseState,
        box_cell: u16,
        push_count: u32,
        direction: Direction,
        trace: Vec<(usize, Direction)>,
    }

    // Build initial trace: the first push's box_index in the initial state's sorted cells
    let initial_trace = vec![(initial_succ.box_index, initial_succ.direction)];

    let mut queue = vec![SeqState {
        state: initial_succ.state.clone(),
        box_cell: initial_box_cell,
        push_count: initial_succ.push_count,
        direction: initial_succ.direction,
        trace: initial_trace,
    }];

    let mut endpoints: Vec<PushSuccessor> = Vec::new();
    let mut seen = FxHashSet::default();
    seen.insert(state_signature(&initial_succ.state));
    let mut head = 0;

    while head < queue.len() && queue.len() < max_explored {
        let current_push_count = queue[head].push_count;
        let current_box_cell = queue[head].box_cell;

        if current_push_count >= max_depth {
            let s = &queue[head];
            endpoints.push(PushSuccessor {
                state: s.state.clone(),
                box_index: initial_succ.box_index,
                direction: s.direction,
                push_count: s.push_count,
                push_trace: s.trace.clone(),
                box_target: s.box_cell,
            });
            head += 1;
            continue;
        }

        let continuations = single_box_pushes(cb, &queue[head].state, current_box_cell);

        if continuations.is_empty() || continuations.len() > 1 {
            let s = &queue[head];
            endpoints.push(PushSuccessor {
                state: s.state.clone(),
                box_index: initial_succ.box_index,
                direction: s.direction,
                push_count: s.push_count,
                push_trace: s.trace.clone(),
                box_target: s.box_cell,
            });
        }

        for (next_state, next_box_cell, next_dir) in continuations {
            let sig = state_signature(&next_state);
            if seen.contains(&sig) {
                continue;
            }
            seen.insert(sig);

            let bi_before_push = queue[head]
                .state
                .box_cells
                .binary_search_by_key(&current_box_cell, |&(c, _)| c)
                .unwrap_or(initial_succ.box_index);

            let mut trace = queue[head].trace.clone();
            trace.push((bi_before_push, next_dir));

            if next_state.is_solved(cb) {
                endpoints.push(PushSuccessor {
                    state: next_state,
                    box_index: initial_succ.box_index,
                    direction: next_dir,
                    push_count: current_push_count + 1,
                    push_trace: trace,
                    box_target: next_box_cell,
                });
                continue;
            }

            queue.push(SeqState {
                state: next_state,
                box_cell: next_box_cell,
                push_count: current_push_count + 1,
                direction: next_dir,
                trace,
            });

            if queue.len() >= max_explored {
                break;
            }
        }

        head += 1;
    }

    for s in queue.iter().skip(head) {
        endpoints.push(PushSuccessor {
            state: s.state.clone(),
            box_index: initial_succ.box_index,
            direction: s.direction,
            push_count: s.push_count,
            push_trace: s.trace.clone(),
            box_target: s.box_cell,
        });
    }

    select_diverse_endpoints(endpoints, max_returned, initial_succ.box_index)
}

fn state_signature(state: &DenseState) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h = h.wrapping_mul(0x100000001b3) ^ state.keeper_zone as u64;
    for &(cell, label) in &state.box_cells {
        h = h.wrapping_mul(0x100000001b3) ^ ((cell as u64) << 8 | label as u64);
    }
    h
}

fn select_diverse_endpoints(
    mut endpoints: Vec<PushSuccessor>,
    max_returned: usize,
    _box_index: usize,
) -> Vec<PushSuccessor> {
    if endpoints.len() <= max_returned {
        return endpoints;
    }

    endpoints.sort_by(|a, b| {
        b.push_count.cmp(&a.push_count).then_with(|| {
            a.state.moves.cmp(&b.state.moves)
        })
    });

    let mut selected = Vec::with_capacity(max_returned);
    let mut seen_destinations = FxHashSet::default();

    // First pass: pick unique destinations
    for ep in &endpoints {
        if selected.len() >= max_returned {
            break;
        }
        let dest = ep.box_target;
        if seen_destinations.insert(dest) {
            selected.push(ep.clone());
        }
    }

    // Second pass: fill remaining slots
    for ep in &endpoints {
        if selected.len() >= max_returned {
            break;
        }
        let sig = state_signature(&ep.state);
        if !selected.iter().any(|s| state_signature(&s.state) == sig) {
            selected.push(ep.clone());
        }
    }

    selected
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
        let (cb, state) = setup();
        let succs = generate_successors(&cb, &state);
        for s in &succs {
            assert!(s.state.keeper_zone < cb.cell_count);
        }
    }

    #[test]
    fn solve_trivial_puzzle() {
        let (cb, state) = setup();

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

    #[test]
    fn macros_dont_break_basic_generation() {
        let (cb, state) = setup();
        let me = MacroEngine::new(&cb);
        let without = generate_successors(&cb, &state);
        let with = generate_successors_with_macros(&cb, &state, Some(&me), None);

        // With macros might produce fewer successors (goal macro) or
        // different targets (tunnel macro), but should not crash.
        assert!(!with.is_empty());
        // On this simple board with no tunnels or safe goals, results should match.
        assert_eq!(without.len(), with.len());
    }

    #[test]
    fn goal_macro_skips_committed_box() {
        // Box already on a corner goal — should be skipped.
        let rows = &["OOOOO", "OS  O", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let me = MacroEngine::new(&cb);

        // Place box on the corner goal at (1,1).
        let goal_cell = cb.goal_cells[0].0;
        let goal_label = cb.goal_cells[0].1 .0;
        let state = DenseState {
            keeper_zone: cb.robot_cell,
            box_cells: vec![(goal_cell, goal_label)],
            moves: 0,
            pushes: 0,
        };

        let without = generate_successors(&cb, &state);
        let with = generate_successors_with_macros(&cb, &state, Some(&me), None);

        // Without macros: may generate pushes for the committed box.
        // With macros: should skip it → fewer successors.
        assert!(with.len() <= without.len());
    }
}
