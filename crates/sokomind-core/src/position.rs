use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Position {
    pub row: i16,
    pub col: i16,
}

impl Position {
    pub const fn new(row: i16, col: i16) -> Self {
        Self { row, col }
    }

    pub const fn offset(self, dir: Direction) -> Self {
        let (dr, dc) = dir.delta();
        Self {
            row: self.row + dr,
            col: self.col + dc,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];

    pub const fn delta(self) -> (i16, i16) {
        match self {
            Direction::Up => (-1, 0),
            Direction::Down => (1, 0),
            Direction::Left => (0, -1),
            Direction::Right => (0, 1),
        }
    }

    pub const fn opposite(self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    pub fn from_char(c: char) -> Option<Direction> {
        match c {
            'U' | 'u' => Some(Direction::Up),
            'D' | 'd' => Some(Direction::Down),
            'L' | 'l' => Some(Direction::Left),
            'R' | 'r' => Some(Direction::Right),
            _ => None,
        }
    }

    pub const fn to_char(self) -> char {
        match self {
            Direction::Up => 'U',
            Direction::Down => 'D',
            Direction::Left => 'L',
            Direction::Right => 'R',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_roundtrip() {
        let pos = Position::new(5, 3);
        for dir in Direction::ALL {
            let moved = pos.offset(dir);
            let back = moved.offset(dir.opposite());
            assert_eq!(back, pos);
        }
    }

    #[test]
    fn direction_from_char() {
        assert_eq!(Direction::from_char('U'), Some(Direction::Up));
        assert_eq!(Direction::from_char('d'), Some(Direction::Down));
        assert_eq!(Direction::from_char('Z'), None);
    }

    #[test]
    fn direction_to_char() {
        assert_eq!(Direction::Up.to_char(), 'U');
        assert_eq!(Direction::Right.to_char(), 'R');
    }

    #[test]
    fn deltas_are_unit_vectors() {
        for dir in Direction::ALL {
            let (dr, dc) = dir.delta();
            assert_eq!(dr.abs() + dc.abs(), 1);
        }
    }
}
