pub mod box_rescheduling;
pub mod bridge_astar;
pub mod move_window;
pub mod permutation_window;
pub mod push_window;

use sokomind_core::position::Direction;

use crate::compiled_board::CompiledBoard;
use crate::deadlock::DeadlockChecker;
use crate::planning::rooms::RoomMap;

/// Summary of improvements applied to a solution.
#[derive(Clone, Debug, Default)]
pub struct ImprovementReport {
    pub pushes_saved: u32,
    pub moves_saved: u32,
}

/// Apply all improvement passes to a push sequence.
///
/// Pass order: push windows → box rescheduling → permutation.
/// The caller converts to full steps and runs move window separately
/// since it operates on the step level. Bridge-A* runs when a RoomMap
/// is provided and doorways exist.
pub fn improve_push_sequence(
    cb: &CompiledBoard,
    deadlocks: &DeadlockChecker,
    pushes: &mut Vec<(usize, Direction)>,
) -> ImprovementReport {
    let mut report = ImprovementReport::default();

    report.pushes_saved += push_window::optimize_pushes(cb, deadlocks, pushes);
    report.pushes_saved += box_rescheduling::reschedule_boxes(cb, deadlocks, pushes);
    report.moves_saved += permutation_window::optimize_permutations(cb, pushes);

    let rooms = RoomMap::analyze(cb);
    if rooms.doorway_count > 0 {
        report.pushes_saved += bridge_astar::bridge_astar_improve(cb, deadlocks, &rooms, pushes);
    }

    report
}
