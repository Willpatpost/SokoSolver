use sokomind_core::board::parse_board;
use sokomind_solver::compiled_board::CompiledBoard;
use sokomind_solver::counters::SearchCounters;
use sokomind_solver::deadlock::DeadlockChecker;
use sokomind_solver::dense_state::DenseState;
use sokomind_solver::heuristic::AssignmentHeuristic;
use sokomind_solver::macros::MacroEngine;
use sokomind_solver::planning::StructuralPlan;
use sokomind_solver::search::successors::generate_successors_with_macros;
use sokomind_solver::zobrist::ZobristKeys;

#[test]
fn grand_hall_diagnostics() {
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
    let initial = DenseState::from_initial(&cb);
    let plan = StructuralPlan::build(&cb);
    let deadlocks = DeadlockChecker::new(&cb);
    let macros = MacroEngine::new(&cb);

    eprintln!("Board: {} floor cells", cb.cell_count);
    eprintln!("Boxes: {}", cb.initial_box_cells.len());
    eprintln!("Goals: {}", cb.goal_cells.len());
    eprintln!("Plan: {}", plan.summary());

    let init_hash = initial.zobrist_hash(&zk);
    let h = heuristic.evaluate(&cb, &initial, init_hash);
    eprintln!("Initial h = {}", h);
    eprintln!(
        "Initial structural boost = {}",
        plan.evaluate_state(&cb, &initial.box_cells)
    );
    eprintln!(
        "Initial doorway occ = {}",
        plan.doorway_occupancy(&cb, &initial.box_cells)
    );

    let mut counters = SearchCounters::default();
    let succs = generate_successors_with_macros(&cb, &initial, Some(&macros), Some(&mut counters));
    eprintln!("\nInitial successors: {}", succs.len());

    let mut deadlocked = 0u32;
    let mut surviving = 0u32;
    for s in &succs {
        if deadlocks.is_deadlocked(&cb, s.state.keeper_zone, &s.state.box_cells, &mut counters) {
            deadlocked += 1;
        } else {
            surviving += 1;
            let sh = s.state.zobrist_hash(&zk);
            let sv = heuristic.evaluate(&cb, &s.state, sh);
            let packing: u32 = s.state.box_cells.iter()
                .filter(|&&(c, l)| cb.goal_matches(c, l))
                .count() as u32;
            let box_label = initial.box_cells[s.box_index].1;
            let target_on_goal = cb.goal_matches(s.box_target, box_label);
            if surviving <= 30 || packing > 0 || target_on_goal {
                let tpos = cb.cell_to_pos(s.box_target);
                let from_pos = cb.cell_to_pos(initial.box_cells[s.box_index].0);
                eprintln!(
                    "  succ box={}(label={}) ({},{})→({},{}) dir={:?} on_goal={}: h={}, packing={}, boost={}",
                    s.box_index, box_label,
                    from_pos.row, from_pos.col, tpos.row, tpos.col,
                    s.direction, target_on_goal,
                    sv, packing,
                    plan.evaluate_state(&cb, &s.state.box_cells),
                );
            }
        }
    }
    eprintln!("Deadlocked: {}, Surviving: {}", deadlocked, surviving);
    eprintln!(
        "Deadlock counters: static={} 2x2={} freeze={} table={} pattern={} goal_commit={} pi_corral={}",
        counters.deadlock_static,
        counters.deadlock_two_by_two,
        counters.deadlock_freeze,
        counters.deadlock_table,
        counters.deadlock_pattern,
        counters.deadlock_goal_commitment,
        counters.deadlock_pi_corral
    );

    // Time individual components
    {
        // Time successor generation (1 call)
        let start = std::time::Instant::now();
        let n_iters = 100;
        for _ in 0..n_iters {
            let _ =
                generate_successors_with_macros(&cb, &initial, Some(&macros), None);
        }
        let elapsed = start.elapsed();
        eprintln!(
            "\nSuccessor generation: {:.3}ms/call ({} iters)",
            elapsed.as_secs_f64() * 1000.0 / n_iters as f64,
            n_iters
        );
    }

    {
        // Time deadlock checking
        let start = std::time::Instant::now();
        let n_iters = 100;
        let succs = generate_successors_with_macros(&cb, &initial, Some(&macros), None);
        for _ in 0..n_iters {
            for s in &succs {
                let _ = deadlocks.is_deadlocked(
                    &cb, s.state.keeper_zone, &s.state.box_cells, &mut counters,
                );
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "Deadlock check: {:.3}ms per {} succs = {:.4}ms/succ ({} iters)",
            elapsed.as_secs_f64() * 1000.0 / n_iters as f64,
            succs.len(),
            elapsed.as_secs_f64() * 1000.0 / (n_iters * succs.len()) as f64,
            n_iters
        );
    }

    {
        // Time heuristic WITH CACHE CLEAR
        let succs = generate_successors_with_macros(&cb, &initial, Some(&macros), None);
        let non_dead: Vec<_> = succs.iter().filter(|s| {
            !deadlocks.is_deadlocked(&cb, s.state.keeper_zone, &s.state.box_cells, &mut counters)
        }).collect();

        let start = std::time::Instant::now();
        let n_iters = 10;
        for _ in 0..n_iters {
            heuristic.clear_cache();
            for s in &non_dead {
                let sh = s.state.zobrist_hash(&zk);
                let _ = heuristic.evaluate(&cb, &s.state, sh);
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "Heuristic (cold cache): {:.3}ms per {} succs = {:.3}ms/succ ({} iters)",
            elapsed.as_secs_f64() * 1000.0 / n_iters as f64,
            non_dead.len(),
            elapsed.as_secs_f64() * 1000.0 / (n_iters * non_dead.len()) as f64,
            n_iters
        );

        // With warm cache
        heuristic.clear_cache();
        // Warm up
        for s in &non_dead {
            let sh = s.state.zobrist_hash(&zk);
            let _ = heuristic.evaluate(&cb, &s.state, sh);
        }
        let start = std::time::Instant::now();
        let n_iters = 100;
        for _ in 0..n_iters {
            for s in &non_dead {
                let sh = s.state.zobrist_hash(&zk);
                let _ = heuristic.evaluate(&cb, &s.state, sh);
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "Heuristic (warm cache): {:.3}ms per {} succs = {:.4}ms/succ ({} iters)",
            elapsed.as_secs_f64() * 1000.0 / n_iters as f64,
            non_dead.len(),
            elapsed.as_secs_f64() * 1000.0 / (n_iters * non_dead.len()) as f64,
            n_iters
        );
    }

    {
        // Time full expansion with cold cache (realistic beam scenario)
        let start = std::time::Instant::now();
        let n_iters = 10;
        for _ in 0..n_iters {
            heuristic.clear_cache();
            let succs =
                generate_successors_with_macros(&cb, &initial, Some(&macros), None);
            for s in &succs {
                if deadlocks.is_deadlocked(
                    &cb, s.state.keeper_zone, &s.state.box_cells, &mut counters,
                ) {
                    continue;
                }
                let sh = s.state.zobrist_hash(&zk);
                let _ = heuristic.evaluate(&cb, &s.state, sh);
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "Full expansion (cold cache): {:.2}ms/expansion ({} iters)",
            elapsed.as_secs_f64() * 1000.0 / n_iters as f64,
            n_iters
        );
    }
}
