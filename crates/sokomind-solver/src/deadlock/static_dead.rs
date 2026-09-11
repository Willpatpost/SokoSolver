use crate::topology::BoardTopology;

/// Check if any box sits on a statically dead cell (corner or dead wall-line
/// with no goal). This is the cheapest deadlock test — O(n_boxes) with a
/// precomputed lookup.
pub fn is_static_deadlock(topo: &BoardTopology, box_cells: &[(u16, u8)]) -> bool {
    box_cells.iter().any(|&(cell, _)| topo.is_dead(cell))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiled_board::CompiledBoard;
    use sokomind_core::board::parse_board;
    use sokomind_core::position::Position;

    #[test]
    fn box_on_dead_corner() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let corner_cell = cb.pos_to_cell(Position::new(1, 1));
        assert!(is_static_deadlock(&topo, &[(corner_cell, 0)]));
    }

    #[test]
    fn box_on_goal_not_dead() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let goal_cell = cb.goal_cells[0].0;
        assert!(!is_static_deadlock(&topo, &[(goal_cell, 0)]));
    }

    #[test]
    fn box_on_open_floor() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let center = cb.pos_to_cell(Position::new(2, 2));
        assert!(!is_static_deadlock(&topo, &[(center, 0)]));
    }

    #[test]
    fn one_dead_among_many() {
        let rows = &["OOOOOOO", "O     O", "O XAR O", "O  Sa O", "OOOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let topo = BoardTopology::analyze(&cb);

        let corner = cb.pos_to_cell(Position::new(1, 1));
        let center = cb.pos_to_cell(Position::new(2, 3));
        assert!(is_static_deadlock(&topo, &[(center, 0), (corner, 1)]));
    }
}
