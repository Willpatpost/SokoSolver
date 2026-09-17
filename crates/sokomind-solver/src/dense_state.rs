use crate::compiled_board::CompiledBoard;
use crate::reachability::canonical_keeper;
use crate::zobrist::ZobristKeys;

/// Compact solver state: keeper position + sorted box positions with labels.
///
/// `keeper_cell` is the exact keeper position (needed for move-optimal search,
/// where walk distance to the next push matters).
///
/// `keeper_zone` is the canonical minimum cell in the keeper's reachable
/// region (used for push-oriented discovery and beam search transposition).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DenseState {
    pub keeper_cell: u16,
    pub keeper_zone: u16,
    pub box_cells: Vec<(u16, u8)>,
    pub moves: u32,
    pub pushes: u32,
}

impl DenseState {
    pub fn from_initial(cb: &CompiledBoard) -> Self {
        let mut box_cells: Vec<(u16, u8)> = cb
            .initial_box_cells
            .iter()
            .map(|&(c, l)| (c, l.0))
            .collect();
        box_cells.sort();

        let keeper_cell = cb.robot_cell;
        let keeper_zone = canonical_keeper(cb, keeper_cell, &box_cells);

        DenseState {
            keeper_cell,
            keeper_zone,
            box_cells,
            moves: 0,
            pushes: 0,
        }
    }

    /// Zobrist hash using keeper zone (for push-oriented beam search).
    pub fn zobrist_hash(&self, zk: &ZobristKeys) -> u64 {
        zk.hash_state(self.keeper_zone, &self.box_cells)
    }

    /// Zobrist hash using exact keeper cell (for move-optimal exact search).
    pub fn zobrist_hash_exact(&self, zk: &ZobristKeys) -> u64 {
        zk.hash_state(self.keeper_cell, &self.box_cells)
    }

    pub fn is_solved(&self, cb: &CompiledBoard) -> bool {
        self.box_cells
            .iter()
            .all(|&(cell, label_group)| cb.goal_matches(cell, label_group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    fn setup() -> CompiledBoard {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        CompiledBoard::from_parsed(&board)
    }

    #[test]
    fn initial_state_from_board() {
        let cb = setup();
        let state = DenseState::from_initial(&cb);
        assert_eq!(state.box_cells.len(), 1);
        assert_eq!(state.moves, 0);
        assert_eq!(state.pushes, 0);
        assert!(!state.is_solved(&cb));
    }

    #[test]
    fn box_cells_sorted() {
        let rows = &["OOOOOOO", "O     O", "O XAR O", "O  Sa O", "OOOOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let state = DenseState::from_initial(&cb);
        let cells: Vec<u16> = state.box_cells.iter().map(|&(c, _)| c).collect();
        let mut sorted = cells.clone();
        sorted.sort();
        assert_eq!(cells, sorted);
    }

    #[test]
    fn solved_detection() {
        let cb = setup();
        let goal_cell = cb.goal_cells[0].0;
        let goal_label = cb.goal_cells[0].1;
        let state = DenseState {
            keeper_cell: 0,
            keeper_zone: 0,
            box_cells: vec![(goal_cell, goal_label.0)],
            moves: 5,
            pushes: 2,
        };
        assert!(state.is_solved(&cb));
    }

    #[test]
    fn hash_consistency() {
        let cb = setup();
        let zk = ZobristKeys::new(&cb, Some(42));
        let state = DenseState::from_initial(&cb);
        let h1 = state.zobrist_hash(&zk);
        let h2 = state.zobrist_hash(&zk);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_states_different_hashes() {
        let cb = setup();
        let zk = ZobristKeys::new(&cb, Some(42));
        let s1 = DenseState::from_initial(&cb);

        let mut s2 = s1.clone();
        let alt_zone = if s1.keeper_zone == 0 { 1 } else { 0 };
        s2.keeper_cell = alt_zone;
        s2.keeper_zone = alt_zone;

        assert_ne!(s1.zobrist_hash(&zk), s2.zobrist_hash(&zk));
    }
}
