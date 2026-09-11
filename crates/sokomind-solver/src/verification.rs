use sokomind_core::board::ParsedBoard;
use sokomind_core::game::{create_snapshot, step_snapshot};
use sokomind_core::position::Direction;

/// Verify a solution by replaying it through the core game engine.
/// Returns Ok(moves, pushes) or Err with the step index that failed.
pub fn verify_solution(
    board: &ParsedBoard,
    steps: &[(Direction, bool)],
) -> Result<(u32, u32), VerificationError> {
    let mut snapshot = create_snapshot(board);

    for (i, &(dir, expected_push)) in steps.iter().enumerate() {
        let transition = step_snapshot(board, &snapshot, dir);
        if !transition.moved {
            return Err(VerificationError::Blocked { step: i });
        }
        if transition.pushed != expected_push {
            return Err(VerificationError::PushMismatch {
                step: i,
                expected: expected_push,
                actual: transition.pushed,
            });
        }
        snapshot = transition.snapshot;
    }

    if !snapshot.solved {
        return Err(VerificationError::NotSolved);
    }

    Ok((snapshot.moves, snapshot.pushes))
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerificationError {
    Blocked {
        step: usize,
    },
    PushMismatch {
        step: usize,
        expected: bool,
        actual: bool,
    },
    NotSolved,
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationError::Blocked { step } => write!(f, "blocked at step {}", step),
            VerificationError::PushMismatch {
                step,
                expected,
                actual,
            } => write!(
                f,
                "push mismatch at step {}: expected={}, actual={}",
                step, expected, actual
            ),
            VerificationError::NotSolved => write!(f, "puzzle not solved after replay"),
        }
    }
}

impl std::error::Error for VerificationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use sokomind_core::board::parse_board;

    #[test]
    fn verify_valid_solution() {
        // OOOOO
        // O  SO
        // O XRO  (box at (2,2), goal at (1,3), robot at (2,3))
        // O   O
        // OOOOO
        // Solution: robot goes up (to 1,3), left (pushes box left to 2,1? no)
        // Actually: push box up (robot at 2,3 goes left to 2,2 pushing box to 2,1... no)
        // Let me think: robot at (2,3), box at (2,2), goal at (1,3)
        // Push box up: robot needs to be below box at (3,2), push up moves box to (1,2)
        // Then push right: robot needs to be left of box at (1,1), push right moves box to (1,3)
        // Robot path: (2,3) → D(3,3) → L(3,2) → L(3,1) → U(2,1) → U(1,1)
        // wait, we need to get robot below box first
        // Robot at (2,3). Box at (2,2).
        // D to (3,3), L to (3,2): robot below box. U pushes box from (2,2) to (1,2).
        // Now box at (1,2), robot at (2,2).
        // Need robot left of box: D to (3,2), L to (3,1), U to (2,1), U to (1,1).
        // R pushes box from (1,2) to (1,3). Solved!
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();

        let steps = vec![
            (Direction::Down, false), // (3,3)
            (Direction::Left, false), // (3,2)
            (Direction::Up, true),    // push box (2,2)->(1,2), robot to (2,2)
            (Direction::Down, false), // (3,2)
            (Direction::Left, false), // (3,1)
            (Direction::Up, false),   // (2,1)
            (Direction::Up, false),   // (1,1)
            (Direction::Right, true), // push box (1,2)->(1,3), robot to (1,2). Solved!
        ];

        let result = verify_solution(&board, &steps);
        assert!(result.is_ok(), "expected valid: {:?}", result);
        let (moves, pushes) = result.unwrap();
        assert_eq!(moves, 8);
        assert_eq!(pushes, 2);
    }

    #[test]
    fn verify_blocked() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let steps = vec![(Direction::Right, false)]; // into wall
        assert_eq!(
            verify_solution(&board, &steps),
            Err(VerificationError::Blocked { step: 0 })
        );
    }

    #[test]
    fn verify_not_solved() {
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let steps = vec![(Direction::Down, false)]; // just walk, not solved
        assert_eq!(
            verify_solution(&board, &steps),
            Err(VerificationError::NotSolved)
        );
    }
}
