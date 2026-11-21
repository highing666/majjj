//! Deterministic wall providers and preset test wall builders.

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use crate::protocol::MatchError;
use crate::seat::Seat;
use crate::tile::Tile;

/// Supplies the initial dealer and per-hand shuffled walls for a `Match`.
pub trait WallProvider {
    /// Selects the initial dealer for hand 1.
    fn initial_dealer(&mut self) -> Seat;

    /// Produces the ordered wall of tiles for the next hand given the canonical `deck`.
    fn next_wall(&mut self, deck: &[Tile]) -> Vec<Tile>;
}

/// Deterministic wall provider backed by `ChaCha8Rng::seed_from_u64`.
#[derive(Debug, Clone)]
pub struct SeededWallProvider {
    rng: ChaCha8Rng,
}

impl SeededWallProvider {
    /// Creates a new deterministic provider from an explicit 64-bit seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    fn gen_range_usize(&mut self, bound: usize) -> usize {
        debug_assert!(bound > 0 && bound <= u32::MAX as usize);
        let bound_u32 = bound as u32;
        let threshold = bound_u32.wrapping_neg() % bound_u32;
        loop {
            let candidate = self.rng.next_u32();
            if candidate >= threshold {
                return (candidate % bound_u32) as usize;
            }
        }
    }
}

impl WallProvider for SeededWallProvider {
    fn initial_dealer(&mut self) -> Seat {
        let idx = (self.rng.next_u32() >> 30) as usize;
        Seat::from_index(idx).unwrap_or(Seat::East)
    }

    fn next_wall(&mut self, deck: &[Tile]) -> Vec<Tile> {
        let mut tiles = deck.to_vec();
        for i in (1..tiles.len()).rev() {
            let j = self.gen_range_usize(i + 1);
            tiles.swap(i, j);
        }
        tiles
    }
}

/// Deterministic wall provider replaying explicit pre-built walls.
#[derive(Debug, Clone)]
pub struct PresetWallProvider {
    initial_dealer: Seat,
    walls: Vec<Vec<Tile>>,
    cursor: usize,
}

impl PresetWallProvider {
    /// Creates a preset provider with an initial dealer and sequence of walls.
    #[must_use]
    pub fn new(initial_dealer: Seat, walls: Vec<Vec<Tile>>) -> Self {
        Self {
            initial_dealer,
            walls,
            cursor: 0,
        }
    }

    /// Creates a preset provider with a single pre-built wall.
    #[must_use]
    pub fn single(initial_dealer: Seat, wall: Vec<Tile>) -> Self {
        Self::new(initial_dealer, vec![wall])
    }
}

impl WallProvider for PresetWallProvider {
    fn initial_dealer(&mut self) -> Seat {
        self.initial_dealer
    }

    fn next_wall(&mut self, _deck: &[Tile]) -> Vec<Tile> {
        let wall = self.walls.get(self.cursor).cloned().unwrap_or_default();
        self.cursor += 1;
        wall
    }
}

/// Builds a valid 136-tile wall from explicit opening hands, head draws, and tail draws,
/// auto-filling all remaining slots with the leftover tiles from the standard 136-tile deck.
///
/// # Errors
///
/// Returns `MatchError::InvalidWall` if hand lengths do not match `(14, 13, 13, 13)`,
/// if `head_draws.len() + tail_draws.len() > 83`, or if any tile kind appears more than 4 times.
pub fn build_preset_wall(
    dealer_14: &[Tile],
    seat1_13: &[Tile],
    seat2_13: &[Tile],
    seat3_13: &[Tile],
    head_draws: &[Tile],
    tail_draws: &[Tile],
) -> Result<Vec<Tile>, MatchError> {
    if dealer_14.len() != 14 {
        return Err(MatchError::InvalidWall(format!(
            "dealer hand must have 14 tiles, got {}",
            dealer_14.len()
        )));
    }
    for (idx, hand) in [seat1_13, seat2_13, seat3_13].iter().enumerate() {
        if hand.len() != 13 {
            return Err(MatchError::InvalidWall(format!(
                "non-dealer hand {} must have 13 tiles, got {}",
                idx + 1,
                hand.len()
            )));
        }
    }
    let remaining_slots = Tile::DECK_SIZE - (14 + 13 * 3);
    if head_draws.len() + tail_draws.len() > remaining_slots {
        return Err(MatchError::InvalidWall(format!(
            "head_draws ({}) + tail_draws ({}) exceeds remaining wall capacity ({remaining_slots})",
            head_draws.len(),
            tail_draws.len(),
        )));
    }

    let mut counts = [0u8; Tile::COUNT];
    for slice in [
        dealer_14, seat1_13, seat2_13, seat3_13, head_draws, tail_draws,
    ] {
        for &tile in slice {
            let c = &mut counts[tile.index()];
            if *c >= 4 {
                return Err(MatchError::InvalidWall(format!(
                    "tile {tile} specified more than 4 times"
                )));
            }
            *c += 1;
        }
    }

    let mut wall = Vec::with_capacity(Tile::DECK_SIZE);
    wall.extend_from_slice(dealer_14);
    wall.extend_from_slice(seat1_13);
    wall.extend_from_slice(seat2_13);
    wall.extend_from_slice(seat3_13);
    wall.extend_from_slice(head_draws);

    for (raw, &used) in counts.iter().enumerate() {
        let tile = Tile::new(raw as u8).expect("loop bounded by Tile::COUNT");
        for _ in used..4 {
            wall.push(tile);
        }
    }

    for &tile in tail_draws.iter().rev() {
        wall.push(tile);
    }

    debug_assert_eq!(wall.len(), Tile::DECK_SIZE);
    Ok(wall)
}

/// Verifies that `wall` has the exact same length and tile multiset as `expected_deck`.
pub(crate) fn validate_wall(wall: &[Tile], expected_deck: &[Tile]) -> Result<(), MatchError> {
    if wall.len() != expected_deck.len() {
        return Err(MatchError::InvalidWall(format!(
            "expected {} tiles, got {}",
            expected_deck.len(),
            wall.len()
        )));
    }

    let mut expected_counts = [0u8; Tile::COUNT];
    for &tile in expected_deck {
        expected_counts[tile.index()] += 1;
    }

    let mut actual_counts = [0u8; Tile::COUNT];
    for &tile in wall {
        actual_counts[tile.index()] += 1;
    }

    if actual_counts != expected_counts {
        return Err(MatchError::InvalidWall(
            "wall tile multiset does not match expected deck".to_string(),
        ));
    }

    Ok(())
}
