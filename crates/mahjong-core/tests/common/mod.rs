//! Shared compact tile-notation parser and table invariant helpers for integration tests.

use mahjong_core::{project_view, Match, MeldView, Ruleset, Seat, Tile, WallProvider};

/// Parses compact whitespace-separated mahjong notation into a `Vec<Tile>`.
///
/// Supported tokens:
/// - Number suits (`1`..`9` followed by `m` = 万, `s` = 条, `p` = 筒), e.g. `"111m 234s 55p"`
/// - Letter honors (`E`/`S`/`W`/`N` winds, `C`/`F`/`B` dragons), e.g. `"EE CFB"`
/// - Numeric honors (`1`..`7` followed by `z` mapping `1z=E`..`7z=B`), e.g. `"1155z"`
#[allow(dead_code)]
pub fn tiles(spec: &str) -> Vec<Tile> {
    let mut out = Vec::new();
    for token in spec.split_whitespace() {
        parse_token(token, &mut out);
    }
    out
}

/// Parses a single tile token (for example `"5m"`, `"E"`, or `"1z"`).
#[allow(dead_code)]
pub fn tile(token: &str) -> Tile {
    let parsed = tiles(token);
    assert_eq!(
        parsed.len(),
        1,
        "expected exactly 1 tile from token {token:?}, got {}",
        parsed.len()
    );
    parsed[0]
}

fn parse_token(token: &str, out: &mut Vec<Tile>) {
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
            return;
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
            _ => panic!(
                "invalid honor tile char {:?} in token {token:?}",
                ch as char
            ),
        };
        out.push(Tile::new(raw).expect("validated honor index in 27..34"));
    }
}

/// Asserts across all four projected seat views that all 136 tiles are conserved.
#[allow(dead_code)]
pub fn assert_136_tile_conservation<R: Ruleset, W: WallProvider>(game: &Match<R, W>) {
    let view = project_view(game, Seat::East);
    let hand_tiles: usize = view.seats.iter().map(|s| usize::from(s.hand_count)).sum();
    let unclaimed_river: usize = view
        .seats
        .iter()
        .flat_map(|s| s.river.iter())
        .filter(|entry| entry.claimed_by.is_none())
        .count();
    let meld_tiles: usize = view
        .seats
        .iter()
        .flat_map(|s| s.melds.iter())
        .map(|meld| match meld {
            MeldView::Pon { .. } | MeldView::Chi { .. } => 3,
            MeldView::ExposedKong { .. }
            | MeldView::AddedKong { .. }
            | MeldView::ConcealedKong { .. } => 4,
        })
        .sum();
    assert_eq!(
        hand_tiles + unclaimed_river + meld_tiles + usize::from(view.wall_remaining),
        136,
        "136-tile conservation violated: hands={hand_tiles}, river={unclaimed_river}, melds={meld_tiles}, wall={}",
        view.wall_remaining
    );
}
