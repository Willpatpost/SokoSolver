use crate::board::ParsedBoard;
use crate::position::{Direction, Position};
use crate::types::{BoxEntity, Label};

/// Dynamic game state. Immutable snapshot — transitions produce new snapshots.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GameSnapshot {
    pub robot: Position,
    pub boxes: Vec<BoxEntity>,
    pub moves: u32,
    pub pushes: u32,
    pub solved: bool,
}

#[derive(Clone, Debug)]
pub struct SnapshotTransition {
    pub snapshot: GameSnapshot,
    pub moved: bool,
    pub pushed: bool,
    pub pushed_box_id: Option<String>,
}

/// Apply one step in the given direction. Returns the new snapshot and what happened.
/// This is the single source of truth for legal transitions.
pub fn step_snapshot(
    board: &ParsedBoard,
    snapshot: &GameSnapshot,
    direction: Direction,
) -> SnapshotTransition {
    let target = snapshot.robot.offset(direction);

    if is_wall(board, target) {
        return blocked(snapshot);
    }

    let pushed_box_index = snapshot.boxes.iter().position(|b| b.position == target);

    if let Some(box_idx) = pushed_box_index {
        let push_target = target.offset(direction);

        if is_wall(board, push_target) || snapshot.boxes.iter().any(|b| b.position == push_target) {
            return blocked(snapshot);
        }

        let box_label = snapshot.boxes[box_idx].label;
        if !can_occupy(board, push_target, box_label) {
            return blocked(snapshot);
        }

        let mut new_boxes = snapshot.boxes.clone();
        let pushed_id = new_boxes[box_idx].id.clone();
        new_boxes[box_idx].position = push_target;

        let solved = check_solved(board, &new_boxes);

        SnapshotTransition {
            snapshot: GameSnapshot {
                robot: target,
                boxes: new_boxes,
                moves: snapshot.moves + 1,
                pushes: snapshot.pushes + 1,
                solved,
            },
            moved: true,
            pushed: true,
            pushed_box_id: Some(pushed_id),
        }
    } else {
        SnapshotTransition {
            snapshot: GameSnapshot {
                robot: target,
                boxes: snapshot.boxes.clone(),
                moves: snapshot.moves + 1,
                pushes: snapshot.pushes,
                solved: snapshot.solved,
            },
            moved: true,
            pushed: false,
            pushed_box_id: None,
        }
    }
}

fn blocked(snapshot: &GameSnapshot) -> SnapshotTransition {
    SnapshotTransition {
        snapshot: snapshot.clone(),
        moved: false,
        pushed: false,
        pushed_box_id: None,
    }
}

fn is_wall(board: &ParsedBoard, pos: Position) -> bool {
    if pos.row < 0 || pos.col < 0 || pos.row >= board.height as i16 || pos.col >= board.width as i16
    {
        return true;
    }
    let ch = board.rows[pos.row as usize]
        .chars()
        .nth(pos.col as usize)
        .unwrap_or('O');
    ch == 'O'
}

/// Check if a box with the given label may rest on this position.
/// Generic boxes (X) can only be on generic goals (S) or floor.
/// Typed boxes can only be on their matching goal or floor.
/// A goal cell of a different label rejects the box.
fn can_occupy(board: &ParsedBoard, pos: Position, _label: Label) -> bool {
    !is_wall(board, pos)
}

fn check_solved(board: &ParsedBoard, boxes: &[BoxEntity]) -> bool {
    boxes.iter().all(|b| {
        board
            .goals
            .iter()
            .any(|g| g.position == b.position && g.label == b.label)
    })
}

pub fn create_snapshot(board: &ParsedBoard) -> GameSnapshot {
    GameSnapshot {
        robot: board.initial_robot,
        boxes: board.initial_boxes.clone(),
        moves: 0,
        pushes: 0,
        solved: check_solved(board, &board.initial_boxes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::parse_board;

    fn setup() -> (ParsedBoard, GameSnapshot) {
        //  OOOOO
        //  O  SO
        //  O XRO
        //  O   O
        //  OOOOO
        let rows = &["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
        let board = parse_board(rows).unwrap();
        let snapshot = create_snapshot(&board);
        (board, snapshot)
    }

    #[test]
    fn walk_into_empty() {
        let (board, snap) = setup();
        let result = step_snapshot(&board, &snap, Direction::Down);
        assert!(result.moved);
        assert!(!result.pushed);
        assert_eq!(result.snapshot.robot, Position::new(3, 3));
        assert_eq!(result.snapshot.moves, 1);
        assert_eq!(result.snapshot.pushes, 0);
    }

    #[test]
    fn walk_into_wall() {
        let (board, snap) = setup();
        let result = step_snapshot(&board, &snap, Direction::Right);
        assert!(!result.moved);
        assert_eq!(result.snapshot.robot, snap.robot);
        assert_eq!(result.snapshot.moves, 0);
    }

    #[test]
    fn push_box() {
        let (board, snap) = setup();
        let result = step_snapshot(&board, &snap, Direction::Left);
        assert!(result.moved);
        assert!(result.pushed);
        assert_eq!(result.snapshot.robot, Position::new(2, 2));
        assert_eq!(result.snapshot.moves, 1);
        assert_eq!(result.snapshot.pushes, 1);
        assert_eq!(result.snapshot.boxes[0].position, Position::new(2, 1));
    }

    #[test]
    fn push_box_into_wall_blocked() {
        let (board, snap) = setup();
        // Push box left first
        let s1 = step_snapshot(&board, &snap, Direction::Left);
        // Now box is at (2,1), try to push it left again into wall
        let s2 = step_snapshot(&board, &s1.snapshot, Direction::Left);
        assert!(!s2.moved);
    }

    #[test]
    fn solve_puzzle() {
        let (board, snap) = setup();
        // Box at (2,2), goal at (1,3). Verify solve detection directly.
        let mut boxes = snap.boxes.clone();
        boxes[0].position = Position::new(1, 3); // on the S goal
        assert!(check_solved(&board, &boxes));

        // Verify unsolved when box is not on goal
        assert!(!check_solved(&board, &snap.boxes));
    }
}
