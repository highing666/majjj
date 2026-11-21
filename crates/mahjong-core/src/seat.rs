//! Table seats and prevailing wind definitions.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Counter-clockwise seat around a 4-player mahjong table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum Seat {
    /// East seat (index `0`).
    East,
    /// South seat (index `1`).
    South,
    /// West seat (index `2`).
    West,
    /// North seat (index `3`).
    North,
}

impl Seat {
    /// All four seats in canonical index order.
    pub const ALL: [Self; 4] = [Self::East, Self::South, Self::West, Self::North];

    /// Returns the zero-based seat index in `0..4`.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::East => 0,
            Self::South => 1,
            Self::West => 2,
            Self::North => 3,
        }
    }

    /// Converts a zero-based index in `0..4` into a `Seat`.
    #[must_use]
    pub const fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::East),
            1 => Some(Self::South),
            2 => Some(Self::West),
            3 => Some(Self::North),
            _ => None,
        }
    }

    /// Returns the next seat counter-clockwise (`East -> South -> West -> North -> East`).
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
            Self::North => Self::East,
        }
    }

    /// Returns the previous seat (`East -> North -> West -> South -> East`).
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::East => Self::North,
            Self::South => Self::East,
            Self::West => Self::South,
            Self::North => Self::West,
        }
    }
}

/// Round wind indicator for a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum Wind {
    /// East wind round.
    East,
    /// South wind round.
    South,
    /// West wind round.
    West,
    /// North wind round.
    North,
}

impl Wind {
    /// All four winds in canonical rotation order.
    pub const ALL: [Self; 4] = [Self::East, Self::South, Self::West, Self::North];

    /// Returns the next wind in rotation.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
            Self::North => Self::East,
        }
    }
}
