//! Shared compact tile-notation and counting helpers for `mahjong-ai` integration tests.

use mahjong_core::Tile;

/// Parses compact whitespace-separated mahjong notation into a `Vec<Tile>`.
#[allow(dead_code)]
pub fn tiles(spec: &str) -> Vec<Tile> {
    let mut out = Vec::new();
    for token in spec.split_whitespace() {
        let bytes = token.as_bytes();
        if let Some((&suffix, digits)) = bytes.split_last() {
            if matches!(suffix, b'm' | b's' | b'p' | b'z') && !digits.is_empty() {
                for &d in digits {
                    let raw = match (suffix, d) {
                        (b'm', b'1'..=b'9') => d - b'1',
                        (b's', b'1'..=b'9') => 9 + (d - b'1'),
                        (b'p', b'1'..=b'9') => 18 + (d - b'1'),
                        (b'z', b'1'..=b'7') => 27 + (d - b'1'),
                        _ => panic!("invalid digit {:?} in tile token {token:?}", d as char),
                    };
                    out.push(Tile::new(raw).expect("validated tile index in 0..34"));
                }
                continue;
            }
        }

        for &ch in bytes {
            let raw = match ch {
                b'E' => 27,
                b'S' => 28,
                b'W' => 29,
                b'N' => 30,
                b'C' => 31,
                b'F' => 32,
                b'B' => 33,
                _ => panic!("invalid honor tile char {:?} in {token:?}", ch as char),
            };
            out.push(Tile::new(raw).expect("validated honor index in 27..34"));
        }
    }
    out
}

/// Parses a single tile token (for example `"5m"`, `"E"`, or `"1z"`).
#[allow(dead_code)]
pub fn tile(token: &str) -> Tile {
    let parsed = tiles(token);
    assert_eq!(parsed.len(), 1, "expected single tile from {token:?}");
    parsed[0]
}

/// Converts a slice of `Tile`s into `[u8; 34]` frequency counts.
#[allow(dead_code)]
pub fn counts_from_tiles(hand: &[Tile]) -> [u8; Tile::COUNT] {
    let mut counts = [0u8; Tile::COUNT];
    for &t in hand {
        counts[t.index()] += 1;
    }
    counts
}

/// Expands `[u8; 34]` frequency counts back into sorted `Vec<Tile>`.
#[allow(dead_code)]
pub fn tiles_from_counts(counts: &[u8; Tile::COUNT]) -> Vec<Tile> {
    let mut out = Vec::new();
    for (idx, &count) in counts.iter().enumerate() {
        let t = Tile::new(idx as u8).expect("index bounded by Tile::COUNT");
        for _ in 0..count {
            out.push(t);
        }
    }
    out
}
