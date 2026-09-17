use crate::dense_state::DenseState;

/// Exact state key for collision-free transposition (proof mode).
/// Uses exact keeper cell (not zone) for move-optimal search, where
/// walk distance to the next push depends on exact keeper position.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactStateKey {
    pub keeper_cell: u16,
    pub box_cells: Vec<(u16, u8)>,
}

impl ExactStateKey {
    pub fn from_dense(state: &DenseState) -> Self {
        ExactStateKey {
            keeper_cell: state.keeper_cell,
            box_cells: state.box_cells.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_states_equal_keys() {
        let s1 = DenseState {
            keeper_cell: 3,
            keeper_zone: 3,
            box_cells: vec![(1, 0), (5, 0)],
            moves: 0,
            pushes: 0,
        };
        let s2 = DenseState {
            keeper_cell: 3,
            keeper_zone: 3,
            box_cells: vec![(1, 0), (5, 0)],
            moves: 10,
            pushes: 5,
        };
        assert_eq!(
            ExactStateKey::from_dense(&s1),
            ExactStateKey::from_dense(&s2)
        );
    }

    #[test]
    fn different_keeper_different_key() {
        let s1 = DenseState {
            keeper_cell: 3,
            keeper_zone: 3,
            box_cells: vec![(1, 0)],
            moves: 0,
            pushes: 0,
        };
        let s2 = DenseState {
            keeper_cell: 4,
            keeper_zone: 4,
            box_cells: vec![(1, 0)],
            moves: 0,
            pushes: 0,
        };
        assert_ne!(
            ExactStateKey::from_dense(&s1),
            ExactStateKey::from_dense(&s2)
        );
    }
}
