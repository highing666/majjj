//! Offline single-binary embedding gate for the 7 built-in `VoicePack` (`角色语音包`) assets.
//!
//! Verifies that `rust-embed` (`EmbeddedStaticAssets`) compiles and serves every canonical
//! VoicePack's `manifest.json` (valid JSON, 46 canonical slots, non-overlapping bounds) and
//! `sprite.m4a` (`audio/mp4`, valid MP4 header, total <= 2.0 MiB budget), and returns `None`
//! (HTTP 404, never SPA `index.html`) for unknown VoicePack paths.

use mahjong_server::persona::{PersonaId, HUMAN_VOICE_PACK_IDS};
use mahjong_server::static_assets::{EmbeddedStaticAssets, StaticAssetError, StaticAssetSource};

const MAX_TOTAL_M4A_BYTES: usize = 2 * 1024 * 1024; // 2.0 MiB budget ceiling

const CANONICAL_VOICE_SLOTS: [&str; 46] = [
    "m1",
    "m2",
    "m3",
    "m4",
    "m5",
    "m6",
    "m7",
    "m8",
    "m9",
    "s1",
    "s2",
    "s3",
    "s4",
    "s5",
    "s6",
    "s7",
    "s8",
    "s9",
    "p1",
    "p2",
    "p3",
    "p4",
    "p5",
    "p6",
    "p7",
    "p8",
    "p9",
    "z1",
    "z2",
    "z3",
    "z4",
    "z5",
    "z6",
    "z7",
    "pong",
    "kong",
    "concealed_kong",
    "zimo",
    "ron",
    "rob_kong",
    "emote_is_that_it",
    "emote_hurry_up",
    "emote_spare_me",
    "emote_nice_play",
    "emote_dealer_rigged",
    "emote_going_big",
];

fn all_canonical_voice_pack_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = PersonaId::ALL
        .iter()
        .map(|persona| persona.voice_pack_id())
        .collect();
    ids.extend(HUMAN_VOICE_PACK_IDS);
    ids
}

#[test]
fn embedded_assets_serve_all_seven_voice_packs_within_budget_and_404_unknown() {
    let assets = EmbeddedStaticAssets;
    let pack_ids = all_canonical_voice_pack_ids();
    assert_eq!(pack_ids.len(), 7);

    let mut total_m4a_bytes = 0usize;

    for pack_id in pack_ids {
        let manifest_uri = format!("/assets/vo/{pack_id}/manifest.json");
        let manifest_asset = assets
            .resolve(&manifest_uri)
            .expect("resolve manifest path")
            .unwrap_or_else(|| {
                panic!(
                    "missing embedded VoicePack manifest {manifest_uri}; run `npm --prefix web run build` first"
                )
            });
        assert_eq!(
            manifest_asset.content_type,
            "application/json; charset=utf-8"
        );
        assert_eq!(manifest_asset.cache_control, "no-cache");

        let manifest: serde_json::Value = serde_json::from_slice(&manifest_asset.bytes)
            .unwrap_or_else(|err| panic!("invalid JSON in {manifest_uri}: {err}"));
        assert_eq!(manifest["pack_id"].as_str(), Some(pack_id));
        assert_eq!(manifest["version"].as_u64(), Some(1));
        assert_eq!(manifest["audio"].as_str(), Some("sprite.m4a"));
        assert_eq!(manifest["sample_rate"].as_u64(), Some(24000));

        let duration = manifest["duration"]
            .as_f64()
            .expect("manifest duration must be finite float");
        assert!(duration > 0.0, "{pack_id} duration must be positive");

        let sprites = manifest["sprites"]
            .as_object()
            .expect("manifest sprites must be a JSON object");
        assert_eq!(
            sprites.len(),
            CANONICAL_VOICE_SLOTS.len(),
            "{pack_id} must contain exactly 46 canonical slots"
        );

        let mut slices = Vec::with_capacity(CANONICAL_VOICE_SLOTS.len());
        for slot in CANONICAL_VOICE_SLOTS {
            let pair = sprites
                .get(slot)
                .and_then(serde_json::Value::as_array)
                .unwrap_or_else(|| panic!("{pack_id} missing canonical slot '{slot}'"));
            assert_eq!(pair.len(), 2, "{pack_id}:{slot} must be [start, duration]");
            let start = pair[0].as_f64().expect("start float");
            let dur = pair[1].as_f64().expect("duration float");
            assert!(
                start >= 0.0 && dur > 0.0,
                "{pack_id}:{slot} invalid slice [{start}, {dur}]"
            );
            assert!(
                start + dur <= duration + 0.001,
                "{pack_id}:{slot} end ({}) exceeds manifest duration ({duration})",
                start + dur
            );
            slices.push((slot, start, dur));
        }

        slices.sort_by(|a, b| a.1.total_cmp(&b.1));
        for pair in slices.windows(2) {
            let (prev_slot, prev_start, prev_dur) = pair[0];
            let (next_slot, next_start, _) = pair[1];
            assert!(
                next_start >= prev_start + prev_dur - 1e-6,
                "{pack_id}: slot '{next_slot}' ({next_start}) overlaps '{prev_slot}' ({})",
                prev_start + prev_dur
            );
        }

        let sprite_uri = format!("/assets/vo/{pack_id}/sprite.m4a");
        let sprite_asset = assets
            .resolve(&sprite_uri)
            .expect("resolve sprite path")
            .unwrap_or_else(|| panic!("missing embedded VoicePack audio {sprite_uri}"));
        assert_eq!(sprite_asset.content_type, "audio/mp4");
        assert!(
            sprite_asset.bytes.len() > 1024,
            "{sprite_uri} must be non-empty audio"
        );
        assert_eq!(
            &sprite_asset.bytes[4..8],
            b"ftyp",
            "{sprite_uri} must start with valid MP4 ftyp box"
        );
        total_m4a_bytes += sprite_asset.bytes.len();
    }

    assert!(
        total_m4a_bytes <= MAX_TOTAL_M4A_BYTES,
        "embedded 7-pack VoicePack total {total_m4a_bytes} bytes exceeds 2.0 MiB budget ({MAX_TOTAL_M4A_BYTES} bytes)"
    );

    // Non-existent VoicePack routes under /assets/vo/ must return None (404), never SPA fallback HTML.
    assert!(assets
        .resolve("/assets/vo/nonexistent/manifest.json")
        .expect("resolve nonexistent manifest")
        .is_none());
    assert!(assets
        .resolve("/assets/vo/nonexistent/sprite.m4a")
        .expect("resolve nonexistent sprite")
        .is_none());

    // Traversal attempts under /assets/vo/ must be rejected.
    assert!(matches!(
        assets.resolve("/assets/vo/../index.html"),
        Err(StaticAssetError::InvalidPath)
    ));
}
