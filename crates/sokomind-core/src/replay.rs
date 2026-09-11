use crate::board::ParsedBoard;
use crate::game::{create_snapshot, step_snapshot, GameSnapshot};
use crate::position::Direction;

pub fn encode_action_log(directions: &[Direction]) -> String {
    directions.iter().map(|d| d.to_char()).collect()
}

pub fn decode_action_log(log: &str) -> Result<Vec<Direction>, ReplayError> {
    log.chars()
        .enumerate()
        .map(|(i, c)| Direction::from_char(c).ok_or(ReplayError::InvalidChar { index: i, char: c }))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplayError {
    InvalidChar { index: usize, char: char },
    BlockedAt { index: usize },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::InvalidChar { index, char } => {
                write!(f, "invalid direction '{}' at index {}", char, index)
            }
            ReplayError::BlockedAt { index } => {
                write!(f, "move blocked at step {}", index)
            }
        }
    }
}

impl std::error::Error for ReplayError {}

pub fn replay_action_log(board: &ParsedBoard, log: &str) -> Result<GameSnapshot, ReplayError> {
    let directions = decode_action_log(log)?;
    let mut snapshot = create_snapshot(board);

    for (i, dir) in directions.iter().enumerate() {
        let transition = step_snapshot(board, &snapshot, *dir);
        if !transition.moved {
            return Err(ReplayError::BlockedAt { index: i });
        }
        snapshot = transition.snapshot;
    }

    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::parse_board;

    #[test]
    fn encode_decode_roundtrip() {
        let dirs = vec![
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ];
        let log = encode_action_log(&dirs);
        assert_eq!(log, "UDLR");
        let decoded = decode_action_log(&log).unwrap();
        assert_eq!(decoded, dirs);
    }

    #[test]
    fn decode_invalid_char() {
        assert_eq!(
            decode_action_log("UDZ"),
            Err(ReplayError::InvalidChar {
                index: 2,
                char: 'Z'
            })
        );
    }

    #[test]
    fn replay_blocked_move() {
        let rows = &["OOO", "ORO", "OOO"];
        let board = parse_board(rows);
        // This board has no boxes, so it should fail to parse
        assert!(board.is_err());
    }

    #[test]
    fn replay_valid_sequence() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let snapshot = replay_action_log(&board, "D").unwrap();
        assert_eq!(snapshot.moves, 1);
        assert_eq!(snapshot.pushes, 0);
    }
}
