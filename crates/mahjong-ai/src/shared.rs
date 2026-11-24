//! Crate-private shared constants and pure math/PRNG helpers for `mahjong-ai`.

use mahjong_core::Tile;
use rand_core::RngCore;

/// Tile indices (`0..34`) for terminals (`1m, 9m, 1s, 9s, 1p, 9p`) and honors (`E..B`).
pub(crate) const TERMINAL_OR_HONOR_INDICES: [usize; 13] =
    [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

/// 34-bit mask covering all valid `Tile` indices (`0..34`).
pub(crate) const TILE_MASK_34: u64 = (1u64 << Tile::COUNT) - 1;

/// Numerically stable logistic sigmoid `1 / (1 + exp(-z))`.
#[inline]
pub(crate) fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        let e = (-z).exp();
        1.0 / (1.0 + e)
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

/// SplitMix64 64-bit avalanche finalizer (without Weyl increment).
#[inline]
pub(crate) const fn splitmix64_mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Full SplitMix64 step (Weyl increment + avalanche finalizer).
#[inline]
pub(crate) const fn splitmix64(z: u64) -> u64 {
    splitmix64_mix(z.wrapping_add(0x9E37_79B9_7F4A_7C15))
}

/// Unbiased uniform integer sampling in `0..bound` using `RngCore::next_u32`.
pub(crate) fn gen_range_usize<R: RngCore + ?Sized>(rng: &mut R, bound: usize) -> usize {
    debug_assert!(bound > 0 && bound <= u32::MAX as usize);
    let bound_u32 = bound as u32;
    let threshold = bound_u32.wrapping_neg() % bound_u32;
    loop {
        let candidate = rng.next_u32();
        if candidate >= threshold {
            return (candidate % bound_u32) as usize;
        }
    }
}
