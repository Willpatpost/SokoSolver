use crate::position::Position;
use crate::types::{BoxEntity, Goal, Label};

/// Static puzzle geometry parsed from row strings. Immutable after construction.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParsedBoard {
    pub width: u16,
    pub height: u16,
    pub rows: Vec<String>,
    pub walls: Vec<Position>,
    pub floor: Vec<Position>,
    pub goals: Vec<Goal>,
    pub initial_robot: Position,
    pub initial_boxes: Vec<BoxEntity>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    NoRobot,
    MultipleRobots,
    NoBoxes,
    LabelMismatch {
        label: char,
        boxes: usize,
        goals: usize,
    },
    RobotOnWall,
    BoxOnWall,
    GoalOnWall,
    DuplicatePosition(Position),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => write!(f, "board is empty"),
            ParseError::NoRobot => write!(f, "no robot (R) found"),
            ParseError::MultipleRobots => write!(f, "multiple robots found"),
            ParseError::NoBoxes => write!(f, "no boxes found"),
            ParseError::LabelMismatch {
                label,
                boxes,
                goals,
            } => {
                write!(f, "label '{}': {} boxes but {} goals", label, boxes, goals)
            }
            ParseError::RobotOnWall => write!(f, "robot is on a wall"),
            ParseError::BoxOnWall => write!(f, "box is on a wall"),
            ParseError::GoalOnWall => write!(f, "goal is on a wall"),
            ParseError::DuplicatePosition(pos) => {
                write!(f, "duplicate entity at ({}, {})", pos.row, pos.col)
            }
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_board(rows: &[&str]) -> Result<ParsedBoard, ParseError> {
    if rows.is_empty() {
        return Err(ParseError::Empty);
    }

    let height = rows.len() as u16;
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0) as u16;

    let padded_rows: Vec<String> = rows
        .iter()
        .map(|r| {
            let mut s = r.to_string();
            while (s.len() as u16) < width {
                s.push('O');
            }
            s
        })
        .collect();

    let mut walls = Vec::new();
    let mut floor = Vec::new();
    let mut goals = Vec::new();
    let mut robot: Option<Position> = None;
    let mut boxes = Vec::new();
    let mut box_counts: std::collections::HashMap<u8, usize> = std::collections::HashMap::new();
    let mut goal_counts: std::collections::HashMap<u8, usize> = std::collections::HashMap::new();

    for (r, row) in padded_rows.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            let pos = Position::new(r as i16, c as i16);
            match ch {
                'O' => {
                    walls.push(pos);
                }
                ' ' => {
                    floor.push(pos);
                }
                'R' => {
                    if robot.is_some() {
                        return Err(ParseError::MultipleRobots);
                    }
                    robot = Some(pos);
                    floor.push(pos);
                }
                'X' => {
                    let label = Label::GENERIC;
                    let id = format!("X:{}", box_counts.entry(0).or_insert(0));
                    *box_counts.entry(0).or_insert(0) += 1;
                    boxes.push(BoxEntity {
                        id,
                        label,
                        position: pos,
                    });
                    floor.push(pos);
                }
                'S' => {
                    goals.push(Goal {
                        label: Label::GENERIC,
                        position: pos,
                    });
                    *goal_counts.entry(0).or_insert(0) += 1;
                    floor.push(pos);
                }
                c if Label::from_box_char(c).is_some() => {
                    let label = Label::from_box_char(c).unwrap();
                    let count = box_counts.entry(label.0).or_insert(0);
                    let id = format!("{}:{}", c, *count);
                    *count += 1;
                    boxes.push(BoxEntity {
                        id,
                        label,
                        position: pos,
                    });
                    floor.push(pos);
                }
                c if Label::from_goal_char(c).is_some() => {
                    let label = Label::from_goal_char(c).unwrap();
                    goals.push(Goal {
                        label,
                        position: pos,
                    });
                    *goal_counts.entry(label.0).or_insert(0) += 1;
                    floor.push(pos);
                }
                _ => {
                    walls.push(pos);
                }
            }
        }
    }

    let initial_robot = robot.ok_or(ParseError::NoRobot)?;
    if boxes.is_empty() {
        return Err(ParseError::NoBoxes);
    }

    // Verify label counts match
    let mut all_labels: std::collections::HashSet<u8> = std::collections::HashSet::new();
    all_labels.extend(box_counts.keys());
    all_labels.extend(goal_counts.keys());

    for &label_id in &all_labels {
        let bc = box_counts.get(&label_id).copied().unwrap_or(0);
        let gc = goal_counts.get(&label_id).copied().unwrap_or(0);
        if bc != gc {
            let label_char = if label_id == 0 {
                'X'
            } else {
                (b'A' + label_id - 1) as char
            };
            return Err(ParseError::LabelMismatch {
                label: label_char,
                boxes: bc,
                goals: gc,
            });
        }
    }

    Ok(ParsedBoard {
        width,
        height,
        rows: padded_rows,
        walls,
        floor,
        goals,
        initial_robot,
        initial_boxes: boxes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_puzzle() {
        let rows = &["OOOOO", "OR XO", "O  SO", "OOOOO"];
        let board = parse_board(rows).unwrap();
        assert_eq!(board.width, 5);
        assert_eq!(board.height, 4);
        assert_eq!(board.initial_robot, Position::new(1, 1));
        assert_eq!(board.initial_boxes.len(), 1);
        assert_eq!(board.goals.len(), 1);
        assert!(board.initial_boxes[0].label.is_generic());
    }

    #[test]
    fn parse_typed_boxes() {
        let rows = &["OOOOO", "ORAaO", "OB bO", "OOOOO"];
        let board = parse_board(rows).unwrap();
        assert_eq!(board.initial_boxes.len(), 2);
        assert_eq!(board.goals.len(), 2);
        assert_eq!(board.initial_boxes[0].label.box_char(), 'A');
        assert_eq!(board.initial_boxes[1].label.box_char(), 'B');
    }

    #[test]
    fn no_robot_error() {
        let rows = &["OOO", "OXO", "OSO", "OOO"];
        assert_eq!(parse_board(rows), Err(ParseError::NoRobot));
    }

    #[test]
    fn label_mismatch_error() {
        let rows = &["OOOOO", "ORAaO", "OA  O", "OOOOO"];
        match parse_board(rows) {
            Err(ParseError::LabelMismatch {
                label,
                boxes,
                goals,
            }) => {
                assert_eq!(label, 'A');
                assert_eq!(boxes, 2);
                assert_eq!(goals, 1);
            }
            other => panic!("expected LabelMismatch, got {:?}", other),
        }
    }

    #[test]
    fn ragged_rows_padded() {
        let rows = &["OOOOO", "OR X", "O  SO", "OOO"];
        let board = parse_board(rows).unwrap();
        assert_eq!(board.width, 5);
        assert!(board.rows.iter().all(|r| r.len() == 5));
    }
}
