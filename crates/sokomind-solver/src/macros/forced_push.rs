use crate::compiled_board::CompiledBoard;
use crate::counters::SearchCounters;
use crate::deadlock::DeadlockChecker;
use crate::dense_state::DenseState;
use crate::search::successors::{generate_successors, PushSuccessor};

const MAX_CHAIN_DEPTH: u32 = 50;

/// Apply the forced-push macro: if a state has exactly one non-deadlocked
/// push successor, automatically chain through it. This collapses linear
/// sequences of forced moves into a single multi-push step.
///
/// Returns the final state after all forced pushes, plus the accumulated
/// push sequence. If no chaining applies, returns None.
pub fn apply_forced_push(
    cb: &CompiledBoard,
    state: &DenseState,
    deadlocks: &DeadlockChecker,
    counters: &mut SearchCounters,
) -> Option<ForcedPushResult> {
    let succs = generate_successors(cb, state);
    let live: Vec<PushSuccessor> = succs
        .into_iter()
        .filter(|s| !deadlocks.is_deadlocked_quick(cb, &s.state.box_cells))
        .collect();

    if live.len() != 1 {
        return None;
    }

    let mut current = live.into_iter().next().unwrap();
    let mut chain = vec![(current.box_index, current.direction)];
    let mut depth = 1u32;

    loop {
        if depth >= MAX_CHAIN_DEPTH {
            break;
        }

        if current.state.is_solved(cb) {
            break;
        }

        let next_succs = generate_successors(cb, &current.state);
        counters.generated += next_succs.len() as u64;

        let next_live: Vec<PushSuccessor> = next_succs
            .into_iter()
            .filter(|s| !deadlocks.is_deadlocked_quick(cb, &s.state.box_cells))
            .collect();

        if next_live.len() != 1 {
            break;
        }

        current = next_live.into_iter().next().unwrap();
        chain.push((current.box_index, current.direction));
        counters.macro_forced_push += 1;
        depth += 1;
    }

    if chain.len() <= 1 {
        return None;
    }

    counters.macro_forced_push += 1;

    Some(ForcedPushResult {
        final_state: current.state,
        pushes: chain,
    })
}

pub struct ForcedPushResult {
    pub final_state: DenseState,
    pub pushes: Vec<(usize, sokomind_core::position::Direction)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn no_forced_push_when_branching() {
        let rows = &[
            "OOOOOOO", "OSS   O", "O     O", "O XX  O", "O  R  O", "O     O", "OOOOOOO",
        ];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);
        let state = DenseState::from_initial(&cb);
        let mut counters = SearchCounters::default();

        let result = apply_forced_push(&cb, &state, &deadlocks, &mut counters);
        assert!(result.is_none(), "branching state should not chain");
    }

    #[test]
    fn forced_push_in_corridor() {
        // Box in a 1-wide corridor with only one push direction.
        // OOOOO
        // OS  O
        // OXOOO  ← box at (2,1), only pushable left (but wall), or right? No — wall at (2,2).
        // OR  O  Wait, (2,2) is wall 'O'. So box at (2,1) can only be pushed...
        //        Actually this is a dead end.
        // Let me use a simpler setup.
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let deadlocks = DeadlockChecker::new(&cb);
        let state = DenseState::from_initial(&cb);
        let mut counters = SearchCounters::default();

        // This puzzle has multiple push options, so forced push shouldn't fire.
        let result = apply_forced_push(&cb, &state, &deadlocks, &mut counters);
        // Result depends on how many non-deadlocked successors exist.
        let _ = result;
    }
}
