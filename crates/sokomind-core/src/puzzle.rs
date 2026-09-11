use serde::{Deserialize, Serialize};

use crate::board::{parse_board, ParseError, ParsedBoard};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Difficulty {
    Tutorial,
    Beginner,
    Intermediate,
    Advanced,
    Expert,
    Master,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleDefinition {
    pub id: String,
    pub title: String,
    pub difficulty: Difficulty,
    pub rows: Vec<String>,
    pub boxes: u16,
    pub hint: Option<String>,
    pub collection: Option<String>,
}

impl PuzzleDefinition {
    pub fn parse_board(&self) -> Result<ParsedBoard, ParseError> {
        let row_refs: Vec<&str> = self.rows.iter().map(|s| s.as_str()).collect();
        parse_board(&row_refs)
    }
}

/// Compute a deterministic fingerprint for a puzzle's board content.
/// Used to invalidate saved progress when a puzzle is edited.
pub fn puzzle_fingerprint(rows: &[String], boxes: u16) -> String {
    let mut hash: u32 = 2166136261; // FNV-1a offset basis
    let input = format!("puzzle-v1:{}:{}", boxes, rows.join("\n"));
    for byte in input.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619); // FNV prime
    }
    format!("puzzle-v1:{:08x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_deterministic() {
        let rows = vec!["OOOOO".into(), "OR XO".into(), "O  SO".into(), "OOOOO".into()];
        let f1 = puzzle_fingerprint(&rows, 1);
        let f2 = puzzle_fingerprint(&rows, 1);
        assert_eq!(f1, f2);
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let rows1 = vec!["OOOOO".into(), "OR XO".into(), "O  SO".into(), "OOOOO".into()];
        let rows2 = vec!["OOOOO".into(), "O RXO".into(), "O  SO".into(), "OOOOO".into()];
        assert_ne!(puzzle_fingerprint(&rows1, 1), puzzle_fingerprint(&rows2, 1));
    }

    #[test]
    fn parse_puzzle_definition() {
        let puzzle = PuzzleDefinition {
            id: "test-1".into(),
            title: "Test Puzzle".into(),
            difficulty: Difficulty::Tutorial,
            rows: vec!["OOOOO".into(), "OR XO".into(), "O  SO".into(), "OOOOO".into()],
            boxes: 1,
            hint: None,
            collection: None,
        };
        let board = puzzle.parse_board().unwrap();
        assert_eq!(board.initial_boxes.len(), 1);
    }
}
