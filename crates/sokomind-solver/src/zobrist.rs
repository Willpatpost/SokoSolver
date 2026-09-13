use crate::compiled_board::CompiledBoard;
use crate::rng::SplitMix64;

/// Pre-computed Zobrist keys for incremental state hashing.
pub struct ZobristKeys {
    box_keys: Vec<Vec<u64>>,
    keeper_keys: Vec<u64>,
    label_groups: usize,
}

const ZOBRIST_SEED: u64 = 0x534F4B4F4D494E44;

impl ZobristKeys {
    pub fn new(cb: &CompiledBoard, seed: Option<u64>) -> Self {
        let mut rng = SplitMix64::new(seed.unwrap_or(ZOBRIST_SEED));
        let cell_count = cb.cell_count as usize;

        let max_label = cb
            .goal_cells
            .iter()
            .chain(cb.initial_box_cells.iter())
            .map(|&(_, l)| l.0 as usize)
            .max()
            .unwrap_or(0);
        let label_groups = max_label + 1;

        let mut box_keys = vec![vec![0u64; cell_count]; label_groups];
        for group in &mut box_keys {
            for key in group.iter_mut() {
                *key = rng.next_u64();
            }
        }

        let mut keeper_keys = vec![0u64; cell_count];
        for key in keeper_keys.iter_mut() {
            *key = rng.next_u64();
        }

        Self {
            box_keys,
            keeper_keys,
            label_groups,
        }
    }

    pub fn box_key(&self, label_group: u8, cell: u16) -> u64 {
        let g = (label_group as usize).min(self.label_groups - 1);
        self.box_keys[g][cell as usize]
    }

    pub fn keeper_key(&self, cell: u16) -> u64 {
        self.keeper_keys[cell as usize]
    }

    /// Compute full hash from scratch for a state.
    pub fn hash_state(&self, keeper_zone: u16, box_cells: &[(u16, u8)]) -> u64 {
        let mut h = self.keeper_key(keeper_zone);
        for &(cell, label_group) in box_cells {
            h ^= self.box_key(label_group, cell);
        }
        h
    }

    /// Hash only the box positions (no keeper zone). Useful for caching
    /// heuristic values which depend only on box placements.
    pub fn hash_boxes(&self, box_cells: &[(u16, u8)]) -> u64 {
        let mut h = 0u64;
        for &(cell, label_group) in box_cells {
            h ^= self.box_key(label_group, cell);
        }
        h
    }

    /// Incremental update: box moved from old_cell to new_cell.
    pub fn update_box_move(&self, hash: u64, label_group: u8, old_cell: u16, new_cell: u16) -> u64 {
        hash ^ self.box_key(label_group, old_cell) ^ self.box_key(label_group, new_cell)
    }

    /// Incremental update: keeper zone changed.
    pub fn update_keeper(&self, hash: u64, old_zone: u16, new_zone: u16) -> u64 {
        hash ^ self.keeper_key(old_zone) ^ self.keeper_key(new_zone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    fn setup() -> (CompiledBoard, ZobristKeys) {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let cb = CompiledBoard::from_parsed(&board);
        let zk = ZobristKeys::new(&cb, Some(42));
        (cb, zk)
    }

    #[test]
    fn deterministic_keys() {
        let (cb, _) = setup();
        let zk1 = ZobristKeys::new(&cb, Some(42));
        let zk2 = ZobristKeys::new(&cb, Some(42));
        for c in 0..cb.cell_count {
            assert_eq!(zk1.keeper_key(c), zk2.keeper_key(c));
            assert_eq!(zk1.box_key(0, c), zk2.box_key(0, c));
        }
    }

    #[test]
    fn different_seeds_different_keys() {
        let (cb, _) = setup();
        let zk1 = ZobristKeys::new(&cb, Some(1));
        let zk2 = ZobristKeys::new(&cb, Some(2));
        let mut same = 0;
        for c in 0..cb.cell_count {
            if zk1.box_key(0, c) == zk2.box_key(0, c) {
                same += 1;
            }
        }
        assert!(same < 2);
    }

    #[test]
    fn incremental_matches_full() {
        let (_, zk) = setup();
        let boxes = vec![(2u16, 0u8), (5, 0)];
        let h1 = zk.hash_state(0, &boxes);

        let boxes2 = vec![(3u16, 0u8), (5, 0)];
        let h2 = zk.hash_state(0, &boxes2);

        let h_inc = zk.update_box_move(h1, 0, 2, 3);
        assert_eq!(h_inc, h2);
    }

    #[test]
    fn keeper_update() {
        let (_, zk) = setup();
        let boxes = vec![(2u16, 0u8)];
        let h1 = zk.hash_state(0, &boxes);
        let h2 = zk.hash_state(1, &boxes);

        let h_inc = zk.update_keeper(h1, 0, 1);
        assert_eq!(h_inc, h2);
    }

    #[test]
    fn collision_free_small_board() {
        let (cb, zk) = setup();
        let mut hashes = std::collections::HashSet::new();
        for c in 0..cb.cell_count {
            for k in 0..cb.cell_count {
                if k == c {
                    continue;
                }
                let h = zk.hash_state(k, &[(c, 0)]);
                assert!(hashes.insert(h), "collision at box={} keeper={}", c, k);
            }
        }
    }
}
