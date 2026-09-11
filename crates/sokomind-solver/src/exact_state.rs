use crate::dense_state::DenseState;

/// Exact state key for collision-free transposition (proof mode).
/// Includes keeper zone + all box positions with labels — no hash collisions possible.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactStateKey {
    pub keeper_zone: u16,
    pub box_cells: Vec<(u16, u8)>,
}

impl ExactStateKey {
    pub fn from_dense(state: &DenseState) -> Self {
        ExactStateKey {
            keeper_zone: state.keeper_zone,
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
            keeper_zone: 3,
            box_cells: vec![(1, 0), (5, 0)],
            moves: 0,
            pushes: 0,
        };
        let s2 = DenseState {
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
            keeper_zone: 3,
            box_cells: vec![(1, 0)],
            moves: 0,
            pushes: 0,
        };
        let s2 = DenseState {
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
