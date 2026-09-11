use serde::{Deserialize, Serialize};

use crate::position::Position;

/// Box and goal label. 0 = generic (X/S), 1-26 = A-Z typed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Label(pub u8);

impl Label {
    pub const GENERIC: Label = Label(0);

    pub fn from_box_char(c: char) -> Option<Label> {
        match c {
            'X' => Some(Label::GENERIC),
            'A'..='N' | 'P'..='Q' | 'T'..='W' | 'Y'..='Z' => Some(Label((c as u8) - b'A' + 1)),
            _ => None,
        }
    }

    pub fn from_goal_char(c: char) -> Option<Label> {
        match c {
            'S' => Some(Label::GENERIC),
            'a'..='n' | 'p'..='q' | 't'..='w' | 'y'..='z' => Some(Label((c as u8) - b'a' + 1)),
            _ => None,
        }
    }

    pub fn box_char(self) -> char {
        if self.0 == 0 {
            'X'
        } else {
            (b'A' + self.0 - 1) as char
        }
    }

    pub fn goal_char(self) -> char {
        if self.0 == 0 {
            'S'
        } else {
            (b'a' + self.0 - 1) as char
        }
    }

    pub fn is_generic(self) -> bool {
        self.0 == 0
    }
}

/// A box on the board with a stable identity.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct BoxEntity {
    /// Stable identifier assigned in row-major parse order (format "LABEL:index").
    pub id: String,
    pub label: Label,
    pub position: Position,
}

/// A goal position on the board.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Goal {
    pub label: Label,
    pub position: Position,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_label() {
        assert!(Label::GENERIC.is_generic());
        assert_eq!(Label::GENERIC.box_char(), 'X');
        assert_eq!(Label::GENERIC.goal_char(), 'S');
    }

    #[test]
    fn typed_label_roundtrip() {
        for c in 'A'..='Z' {
            if let Some(label) = Label::from_box_char(c) {
                assert_eq!(label.box_char(), c);
                let goal_c = label.goal_char();
                assert_eq!(Label::from_goal_char(goal_c), Some(label));
            }
        }
    }

    #[test]
    fn reserved_chars_rejected() {
        assert!(Label::from_box_char('O').is_none()); // wall
        assert!(Label::from_box_char('R').is_none()); // robot
        assert!(Label::from_box_char('S').is_none()); // goal, not a box
        assert!(Label::from_goal_char('o').is_none());
        assert!(Label::from_goal_char('r').is_none());
        assert!(Label::from_goal_char('x').is_none());
        assert!(Label::from_goal_char('s').is_none());
    }
}
