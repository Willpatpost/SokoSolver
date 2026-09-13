use sokomind_core::board::parse_board;
use sokomind_solver::budget::Budget;
use sokomind_solver::compiled_board::CompiledBoard;
use sokomind_solver::config::SolverLimits;
use sokomind_solver::counters::SearchCounters;
use sokomind_solver::deadlock::DeadlockChecker;
use sokomind_solver::dense_state::DenseState;
use sokomind_solver::goal_commitment::GoalCommitmentDetector;
use sokomind_solver::heuristic::AssignmentHeuristic;
use sokomind_solver::macros::MacroEngine;
use sokomind_solver::planning::StructuralPlan;
use sokomind_solver::search::successors::{generate_successors_committed, generate_successors_with_macros};
use sokomind_solver::zobrist::ZobristKeys;

#[test]
fn beam_trace() {
    let rows: Vec<&str> = vec![
        "OOOOOOOOOOOOOOO",
        "OaSS   S   SSbO",
        "OSCS  OOO  SDSO",
        "OX X  OOO  X XO",
        "O     OOO     O",
        "OOOO   X   OOOO",
        "O      O      O",
        "O G hOOOOOH g O",
        "O      O      O",
        "OOO         OOO",
        "OOO   X X   OOO",
        "OOOOOOOROOOOOOO",
        "O B X X X X A O",
        "O Sc       dS O",
        "OOOOOOOOOOOOOOO",
    ];
    let board = parse_board(&rows).unwrap();
    let cb = CompiledBoard::from_parsed(&board);
    let zk = ZobristKeys::new(&cb, Some(42));
    let mut heuristic = AssignmentHeuristic::new(&cb);
    let macros = MacroEngine::new(&cb);
    let plan = StructuralPlan::build(&cb);
    let commitment = GoalCommitmentDetector::new(&cb);

    eprintln!("Plan: {}", plan.summary());
    eprintln!("Room count: {}", plan.room_count());
    eprintln!("Doorway count: {}", plan.doorway_count());
    eprintln!("Commitment detector has_potential: {}", commitment.has_potential());

    let initial = DenseState::from_initial(&cb);

    let init_box_hash = zk.hash_boxes(&initial.box_cells);
    let h0 = heuristic.fast_evaluate(&cb, &initial, init_box_hash);
    eprintln!("Initial h (fast): {}", h0);

    let h0_full = heuristic.evaluate(&cb, &initial, init_box_hash);
    eprintln!("Initial h (full): {}", h0_full);

    let succs = generate_successors_with_macros(&cb, &initial, Some(&macros), None);
    eprintln!("Initial successors: {}", succs.len());

    let committed = commitment.find_committed_boxes(&cb, &initial.box_cells);
    let succs_committed = generate_successors_committed(&cb, &initial, Some(&macros), None, committed);
    eprintln!("Initial successors with commitment: {} (committed mask: {:b})", succs_committed.len(), committed);

    eprintln!("\nGoals on board:");
    for (i, &(cell, label)) in cb.goal_cells.iter().enumerate() {
        let pos = cb.cell_to_pos(cell);
        let is_corner = {
            use sokomind_core::position::Direction;
            use sokomind_solver::compiled_board::INVALID_CELL;
            let up = cb.neighbor(cell, Direction::Up) == INVALID_CELL;
            let down = cb.neighbor(cell, Direction::Down) == INVALID_CELL;
            let left = cb.neighbor(cell, Direction::Left) == INVALID_CELL;
            let right = cb.neighbor(cell, Direction::Right) == INVALID_CELL;
            (up && left) || (up && right) || (down && left) || (down && right)
        };
        eprintln!("  goal[{}]: cell={} pos=({},{}) label={} corner={}", i, cell, pos.row, pos.col, label.0, is_corner);
    }

    eprintln!("\nBoxes in initial state:");
    for (i, &(cell, label)) in initial.box_cells.iter().enumerate() {
        let pos = cb.cell_to_pos(cell);
        let on_goal = cb.goal_matches(cell, label);
        eprintln!("  box[{}]: cell={} pos=({},{}) label={} on_goal={}", i, cell, pos.row, pos.col, label, on_goal);
    }

    eprintln!("\nRoom assignments for boxes:");
    if plan.is_active() {
        let rooms = plan.rooms();
        for (i, &(cell, _)) in initial.box_cells.iter().enumerate() {
            let room = rooms.cell_room(cell);
            let door = rooms.is_doorway(cell);
            eprintln!("  box[{}]: room={:?} doorway={}", i, room, door);
        }
    }
}
