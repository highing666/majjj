//! Standard 34-kind tile representation and 136-tile deck helpers.

use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

/// Suit or honor category of a mahjong tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Suit {
    /// Characters (`0..=8`, 一万 through 九万).
    Man,
    /// Bamboos (`9..=17`, 一条 through 九条).
    Bamboo,
    /// Dots (`18..=26`, 一筒 through 九筒).
    Dot,
    /// Winds (`27..=30`, 东, 南, 西, 北).
    Wind,
    /// Dragons (`31..=33`, 中, 发, 白).
    Dragon,
}

/// Canonical mahjong tile kind encoded continuously in `0..34`.
///
/// Mapping:
/// - `0..=8`: 一万..=九万
/// - `9..=17`: 一条..=九条
/// - `18..=26`: 一筒..=九筒
/// - `27..=30`: 东, 南, 西, 北
/// - `31..=33`: 中, 发, 白
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
pub struct Tile(u8);

impl Tile {
    /// Total number of distinct tile kinds.
    pub const COUNT: usize = 34;

    /// Total number of tiles in a standard 4-copy deck.
    pub const DECK_SIZE: usize = 136;

    /// Constructs a validated `Tile` from a raw index in `0..34`.
    #[must_use]
    pub const fn new(raw: u8) -> Option<Self> {
        if raw < Self::COUNT as u8 {
            Some(Self(raw))
        } else {
            None
        }
    }

    /// Returns the raw `u8` index in `0..34`.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Returns the `usize` index in `0..34` for `[u8; 34]` frequency arrays.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Iterates over all 34 distinct tile kinds in canonical order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT as u8).map(Self)
    }

    /// Returns the suit category of this tile.
    #[must_use]
    pub const fn suit(self) -> Suit {
        match self.0 {
            0..=8 => Suit::Man,
            9..=17 => Suit::Bamboo,
            18..=26 => Suit::Dot,
            27..=30 => Suit::Wind,
            _ => Suit::Dragon,
        }
    }

    /// Returns `Some(1..=9)` for number tiles, or `None` for honor tiles.
    #[must_use]
    pub const fn rank(self) -> Option<u8> {
        if self.0 < 27 {
            Some((self.0 % 9) + 1)
        } else {
            None
        }
    }

    /// Returns `true` if this tile is an honor tile (wind or dragon).
    #[must_use]
    pub const fn is_honor(self) -> bool {
        self.0 >= 27
    }

    /// Returns `true` if this tile is a terminal (`1` or `9` of a number suit).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self.0, 0 | 8 | 9 | 17 | 18 | 26)
    }

    /// Returns `true` if this tile is either a terminal or an honor tile.
    #[must_use]
    pub const fn is_terminal_or_honor(self) -> bool {
        self.is_terminal() || self.is_honor()
    }

    /// Builds the standard 136-tile deck (4 copies of each tile `0..34`).
    #[must_use]
    pub fn standard_deck_136() -> Vec<Self> {
        let mut deck = Vec::with_capacity(Self::DECK_SIZE);
        for tile in Self::all() {
            for _ in 0..4 {
                deck.push(tile);
            }
        }
        deck
    }
}

impl Serialize for Tile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Tile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u8::deserialize(deserializer)?;
        Self::new(raw).ok_or_else(|| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Unsigned(u64::from(raw)),
                &"a tile index in 0..34",
            )
        })
    }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0..=8 => write!(f, "{}m", self.0 + 1),
            9..=17 => write!(f, "{}s", self.0 - 8),
            18..=26 => write!(f, "{}p", self.0 - 17),
            27 => f.write_str("E"),
            28 => f.write_str("S"),
            29 => f.write_str("W"),
            30 => f.write_str("N"),
            31 => f.write_str("C"),
            32 => f.write_str("F"),
            _ => f.write_str("B"),
        }
    }
}
