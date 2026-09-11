pub mod move_window;
pub mod permutation_window;
pub mod push_window;

use sokomind_core::position::Direction;

use crate::compiled_board::CompiledBoard;
use crate::deadlock::DeadlockChecker;

/// Summary of improvements applied to a solution.
#[derive(Clone, Debug, Default)]
pub struct ImprovementReport {
    pub pushes_saved: u32,
    pub moves_saved: u32,
}

/// Apply all improvement passes to a push sequence, returning a
/// (possibly shorter) push sequence and a full step sequence.
///
/// Pass order: push windows → permutation → (the caller converts to
/// full steps and runs move window separately since it operates on
/// the step level).
pub fn improve_push_sequence(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    pushes: &mut Vec<(usize, Direction)>,
) -> ImprovementReport {
    let mut report = ImprovementReport::default();

    report.pushes_saved += push_window::optimize_pushes(cb, deadlocks, pushes);
    report.moves_saved += permutation_window::optimize_permutations(cb, pushes);

    report
}
