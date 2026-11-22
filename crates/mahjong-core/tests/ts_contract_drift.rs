// `ts-rs` drift verification legitimately compares generated files on disk inside a temp dir.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mahjong_core::{
    build_preset_wall, project_event, project_view, DiscardEntry, HandOutcome, HandSettlement,
    KongKind, KongTransfer, Match, MatchError, MatchMode, MatchSummary, MeldView, PlayerAction,
    PlayerEvent, PlayerEventKind, PlayerHandView, PlayerView, PresetWallProvider, PublicPhase,
    PublicSeatView, RevealedHand, RevealedMeld, RuleProfile, ScoreItem, Seat, SeededWallProvider,
    Suit, Tile, TuidaohuRuleset, TurnSource, TurnView, WinKind, Wind, WinnerSettlement,
};
use ts_rs::TS;

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let id = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), id));
        if path.exists() {
            fs::remove_dir_all(&path).expect("clean stale temp dir");
        }
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn generated_bindings_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ parent")
        .parent()
        .expect("repo root")
        .join("web/src/lib/protocol/generated")
}

fn export_all_contract_types(out_dir: &Path) {
    // Export every root contract type plus standalone contract enums (`TurnSource`, `Suit`, `MatchError`)
    // that are not reachable solely through `PlayerView` and `PlayerEvent`.
    PlayerView::export_all_to(out_dir).expect("export PlayerView bindings");
    PlayerEvent::export_all_to(out_dir).expect("export PlayerEvent bindings");
    TurnSource::export_all_to(out_dir).expect("export TurnSource binding");
    Suit::export_all_to(out_dir).expect("export Suit binding");
    MatchError::export_all_to(out_dir).expect("export MatchError binding");
}

fn collect_ts_files(dir: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(dir).expect("read bindings directory") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .expect("utf-8 filename")
                .to_owned();
            let contents = fs::read_to_string(&path).expect("read generated .ts file");
            files.insert(name, contents);
        }
    }
    files
}

fn sync_bindings_dir(src_dir: &Path, dest_dir: &Path) {
    fs::create_dir_all(dest_dir).expect("create generated bindings dir");
    let src_files = collect_ts_files(src_dir);
    for (name, contents) in &src_files {
        fs::write(dest_dir.join(name), contents).expect("write generated binding file");
    }
}

#[test]
fn ts_rs_bindings_match_committed_files_with_zero_drift() {
    let temp = TempDirGuard::new("majjj_ts_contract_drift");
    export_all_contract_types(temp.path());

    let expected_files = collect_ts_files(temp.path());
    let expected_names: Vec<&str> = expected_files.keys().map(String::as_str).collect();
    assert_eq!(
        expected_names,
        vec![
            "DiscardEntry.ts",
            "HandOutcome.ts",
            "HandSettlement.ts",
            "KongKind.ts",
            "KongTransfer.ts",
            "MatchError.ts",
            "MatchMode.ts",
            "MatchSummary.ts",
            "MeldView.ts",
            "PlayerAction.ts",
            "PlayerEvent.ts",
            "PlayerEventKind.ts",
            "PlayerHandView.ts",
            "PlayerView.ts",
            "PublicPhase.ts",
            "PublicSeatView.ts",
            "RevealedHand.ts",
            "RevealedMeld.ts",
            "RuleProfile.ts",
            "ScoreItem.ts",
            "Seat.ts",
            "Suit.ts",
            "Tile.ts",
            "TurnSource.ts",
            "TurnView.ts",
            "WinKind.ts",
            "Wind.ts",
            "WinnerSettlement.ts",
        ],
        "exact set of 28 exported contract TypeScript files"
    );

    // Every JSON integer (`u64` seq fields) must export as TS `number` rather than `bigint`,
    // matching `serde_json` / `JSON.parse` / `JSON.stringify`.
    for (name, contents) in &expected_files {
        assert!(
            !contents.contains("bigint"),
            "generated {name} must not use bigint over JSON WebSocket:\n{contents}"
        );
    }

    let committed_dir = generated_bindings_dir();
    if std::env::var_os("UPDATE_TS_BINDINGS").is_some_and(|v| v == "1") {
        sync_bindings_dir(temp.path(), &committed_dir);
    }

    assert!(
        committed_dir.exists(),
        "committed bindings directory {} does not exist (run with UPDATE_TS_BINDINGS=1 to generate)",
        committed_dir.display()
    );

    let actual_files = collect_ts_files(&committed_dir);
    for name in &expected_names {
        assert!(
            actual_files.contains_key(*name),
            "missing core TypeScript binding {name} in {}",
            committed_dir.display()
        );
    }

    for (name, expected_contents) in &expected_files {
        let actual_contents = actual_files
            .get(name)
            .unwrap_or_else(|| panic!("missing {name} in {}", committed_dir.display()));
        assert_eq!(
            actual_contents, expected_contents,
            "TypeScript contract drift detected in {name}; run UPDATE_TS_BINDINGS=1 cargo test -p mahjong-server --test ts_contract_drift"
        );
    }
}

fn t(raw: u8) -> Tile {
    Tile::new(raw).expect("valid tile index")
}

#[test]
fn rich_contract_types_roundtrip_serde_json_without_loss() {
    // 1. Tile validation + primitives + TurnSource + Suit + KongKind + WinKind.
    assert!(serde_json::from_str::<Tile>("34").is_err());
    assert!(serde_json::from_str::<Tile>("-1").is_err());
    for tile in Tile::all() {
        let json = serde_json::to_string(&tile).unwrap();
        let back: Tile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tile);
    }
    for suit in [Suit::Man, Suit::Bamboo, Suit::Dot, Suit::Wind, Suit::Dragon] {
        let back: Suit = serde_json::from_str(&serde_json::to_string(&suit).unwrap()).unwrap();
        assert_eq!(back, suit);
    }
    for source in [
        TurnSource::InitialDealer,
        TurnSource::NormalDraw,
        TurnSource::ReplacementDraw,
        TurnSource::AfterMeld,
    ] {
        let back: TurnSource =
            serde_json::from_str(&serde_json::to_string(&source).unwrap()).unwrap();
        assert_eq!(back, source);
    }
    for kind in [
        KongKind::ExposedKong,
        KongKind::AddedKong,
        KongKind::ConcealedKong,
    ] {
        let back: KongKind = serde_json::from_str(&serde_json::to_string(&kind).unwrap()).unwrap();
        assert_eq!(back, kind);
    }
    for win_kind in [WinKind::Tsumo, WinKind::Ron, WinKind::RobKongHu] {
        let back: WinKind =
            serde_json::from_str(&serde_json::to_string(&win_kind).unwrap()).unwrap();
        assert_eq!(back, win_kind);
    }

    // 2. All PlayerAction and MatchError variants.
    let all_actions = vec![
        PlayerAction::Discard {
            tile: t(0),
            from_drawn: true,
        },
        PlayerAction::Discard {
            tile: t(33),
            from_drawn: false,
        },
        PlayerAction::Tsumo,
        PlayerAction::Ron { tile: t(8) },
        PlayerAction::RobKongHu { tile: t(17) },
        PlayerAction::Pon { tile: t(26) },
        PlayerAction::ExposedKong { tile: t(27) },
        PlayerAction::AddedKong { tile: t(28) },
        PlayerAction::ConcealedKong { tile: t(29) },
        PlayerAction::Chi {
            tile: t(4),
            with: [t(3), t(5)],
        },
        PlayerAction::Pass,
        PlayerAction::ConfirmNextHand,
        PlayerAction::EndMatch,
    ];
    for action in &all_actions {
        let json = serde_json::to_string(action).unwrap();
        let back: PlayerAction = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, action);
    }

    let all_errors = vec![
        MatchError::IllegalAction {
            seat: Seat::South,
            action: PlayerAction::Tsumo,
        },
        MatchError::WrongPhase {
            seat: Seat::West,
            action: PlayerAction::EndMatch,
        },
        MatchError::NotYourTurn { seat: Seat::North },
        MatchError::AlreadyConfirmed { seat: Seat::East },
        MatchError::InvalidWall("expected 136 tiles, got 10".to_string()),
        MatchError::MatchAlreadyFinished,
    ];
    for err in &all_errors {
        let json = serde_json::to_string(err).unwrap();
        let back: MatchError = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, err);
    }

    // 3. Rich populated HandSettlement (Win with multi-ron, kong transfers, all RevealedMeld variants)
    //    plus DrawGame settlement and MatchSummary.
    let revealed_hands = [
        RevealedHand {
            seat: Seat::East,
            closed_tiles: vec![t(0), t(1), t(2), t(31), t(31)],
            drawn_tile: Some(t(31)),
            melds: vec![
                RevealedMeld::Pon {
                    tile: t(9),
                    from_seat: Seat::South,
                    seq: 12,
                },
                RevealedMeld::ExposedKong {
                    tile: t(18),
                    from_seat: Seat::West,
                    seq: 20,
                },
                RevealedMeld::AddedKong {
                    tile: t(27),
                    from_seat: Seat::North,
                    seq: 25,
                },
            ],
        },
        RevealedHand {
            seat: Seat::South,
            closed_tiles: vec![t(3), t(4), t(5), t(6), t(7), t(8), t(10), t(10)],
            drawn_tile: None,
            melds: vec![
                RevealedMeld::ConcealedKong {
                    tile: t(32),
                    seq: 30,
                },
                RevealedMeld::Chi {
                    tiles: [t(12), t(13), t(14)],
                    called_tile: t(13),
                    from_seat: Seat::East,
                    seq: 34,
                },
            ],
        },
        RevealedHand {
            seat: Seat::West,
            closed_tiles: vec![t(15); 13],
            drawn_tile: None,
            melds: Vec::new(),
        },
        RevealedHand {
            seat: Seat::North,
            closed_tiles: vec![t(20); 13],
            drawn_tile: None,
            melds: Vec::new(),
        },
    ];

    let win_settlement = HandSettlement {
        wind_round: Wind::South,
        hand_number: 3,
        dealer_streak: 2,
        dealer: Seat::East,
        outcome: HandOutcome::Win {
            winners: vec![
                WinnerSettlement {
                    seat: Seat::South,
                    win_kind: WinKind::RobKongHu,
                    items: vec![
                        ScoreItem {
                            code: "menqing".to_string(),
                            label: "门前清".to_string(),
                            fan: 1,
                        },
                        ScoreItem {
                            code: "rob_kong_hu".to_string(),
                            label: "抢杠胡".to_string(),
                            fan: 1,
                        },
                    ],
                    total_fan: 2,
                    base_points: 4,
                },
                WinnerSettlement {
                    seat: Seat::West,
                    win_kind: WinKind::Ron,
                    items: vec![ScoreItem {
                        code: "basic_hu".to_string(),
                        label: "基本胡".to_string(),
                        fan: 0,
                    }],
                    total_fan: 0,
                    base_points: 1,
                },
            ],
            discarder: Some(Seat::East),
            winning_tile: t(27),
        },
        kong_transfers: vec![
            KongTransfer {
                kind: KongKind::ExposedKong,
                actor: Seat::East,
                from_seat: Some(Seat::West),
                tile: t(18),
                score_deltas: [2, 0, -2, 0],
            },
            KongTransfer {
                kind: KongKind::ConcealedKong,
                actor: Seat::South,
                from_seat: None,
                tile: t(32),
                score_deltas: [-2, 6, -2, -2],
            },
        ],
        score_deltas: [-5, 10, -3, -2],
        new_scores: [95, 110, 97, 98],
        revealed_hands: revealed_hands.clone(),
    };
    let settlement_json = serde_json::to_string(&win_settlement).unwrap();
    let settlement_back: HandSettlement = serde_json::from_str(&settlement_json).unwrap();
    assert_eq!(settlement_back, win_settlement);

    let draw_settlement = HandSettlement {
        wind_round: Wind::East,
        hand_number: 4,
        dealer_streak: 0,
        dealer: Seat::North,
        outcome: HandOutcome::DrawGame,
        kong_transfers: Vec::new(),
        score_deltas: [0; 4],
        new_scores: [100; 4],
        revealed_hands,
    };
    assert_eq!(
        serde_json::from_str::<HandSettlement>(&serde_json::to_string(&draw_settlement).unwrap())
            .unwrap(),
        draw_settlement
    );

    let summary = MatchSummary {
        mode: MatchMode::FreeTable,
        hands_played: 12,
        initial_scores: [0; 4],
        final_scores: [14, -6, 4, -12],
        net_deltas: [14, -6, 4, -12],
    };
    assert_eq!(
        serde_json::from_str::<MatchSummary>(&serde_json::to_string(&summary).unwrap()).unwrap(),
        summary
    );

    // 4. Rich populated PlayerView (with melds, river entries, settlement, summary)
    //    and all PlayerEventKind variants from live Match runs + synthetic coverage.
    let rich_view = PlayerView {
        viewer: Seat::East,
        mode: MatchMode::FreeTable,
        rule_profile: RuleProfile {
            name: "Tuidaohu".to_string(),
            allow_chi: false,
            allow_four_of_kind_seven_pairs: true,
            allow_multi_ron: true,
            fan_cap: 4,
            wall_size: 136,
            dead_wall_size: 0,
            starting_scores_east_only: [100; 4],
            starting_scores_free_table: [0; 4],
        },
        phase: PublicPhase::BetweenHands,
        hand: PlayerHandView {
            closed_tiles: vec![t(0), t(1), t(2)],
            drawn_tile: Some(t(3)),
        },
        seats: [
            PublicSeatView {
                seat: Seat::East,
                score: 10,
                hand_count: 4,
                river: vec![DiscardEntry {
                    tile: t(11),
                    is_tsumogiri: true,
                    claimed_by: Some(Seat::South),
                    seq: 17,
                }],
                melds: vec![
                    MeldView::Pon {
                        tile: t(9),
                        from_seat: Seat::South,
                        seq: 12,
                    },
                    MeldView::ExposedKong {
                        tile: t(18),
                        from_seat: Seat::West,
                        seq: 20,
                    },
                    MeldView::AddedKong {
                        tile: t(27),
                        from_seat: Seat::North,
                        seq: 25,
                    },
                    MeldView::ConcealedKong {
                        tile: Some(t(31)),
                        seq: 29,
                    },
                ],
                last_draw_or_meld_seq: 29,
            },
            PublicSeatView {
                seat: Seat::South,
                score: -4,
                hand_count: 10,
                river: Vec::new(),
                melds: vec![
                    MeldView::ConcealedKong {
                        tile: None,
                        seq: 30,
                    },
                    MeldView::Chi {
                        tiles: [t(12), t(13), t(14)],
                        called_tile: t(13),
                        from_seat: Seat::East,
                        seq: 34,
                    },
                ],
                last_draw_or_meld_seq: 34,
            },
            PublicSeatView {
                seat: Seat::West,
                score: -3,
                hand_count: 13,
                river: Vec::new(),
                melds: Vec::new(),
                last_draw_or_meld_seq: 10,
            },
            PublicSeatView {
                seat: Seat::North,
                score: -3,
                hand_count: 13,
                river: Vec::new(),
                melds: Vec::new(),
                last_draw_or_meld_seq: 11,
            },
        ],
        wall_remaining: 42,
        turn: TurnView {
            wind_round: Wind::South,
            hand_number: 3,
            dealer_streak: 2,
            dealer: Seat::East,
            active_seat: None,
        },
        legal_actions: vec![PlayerAction::ConfirmNextHand, PlayerAction::EndMatch],
        last_settlement: Some(win_settlement.clone()),
        ready_seats: Some([true, false, true, false]),
        match_summary: Some(summary),
        seq: 42,
    };
    let view_json = serde_json::to_string(&rich_view).unwrap();
    let view_value: serde_json::Value = serde_json::from_str(&view_json).unwrap();
    assert!(view_value["seq"].is_u64());
    let view_back: PlayerView = serde_json::from_str(&view_json).unwrap();
    assert_eq!(view_back, rich_view);

    // Drive a real Match through discards/draws/settlement/confirmation/EndMatch and roundtrip every event & view.
    let game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        SeededWallProvider::new(21),
    )
    .unwrap();
    for ev in game.events() {
        for seat in Seat::ALL {
            let pe = project_event(ev, seat);
            let pe_back: PlayerEvent =
                serde_json::from_str(&serde_json::to_string(&pe).unwrap()).unwrap();
            assert_eq!(pe_back, pe);
        }
    }

    let synthetic_events = [
        PlayerEvent {
            seq: 10,
            kind: PlayerEventKind::TileDrawn {
                seat: Seat::South,
                tile: None,
                is_replacement: true,
                wall_remaining: 50,
            },
        },
        PlayerEvent {
            seq: 11,
            kind: PlayerEventKind::TileDiscarded {
                seat: Seat::South,
                tile: t(15),
                is_tsumogiri: true,
            },
        },
        PlayerEvent {
            seq: 12,
            kind: PlayerEventKind::AddedKongAttempted {
                seat: Seat::East,
                tile: t(27),
            },
        },
        PlayerEvent {
            seq: 13,
            kind: PlayerEventKind::MeldDeclared {
                seat: Seat::East,
                meld: MeldView::ConcealedKong {
                    tile: None,
                    seq: 13,
                },
                kong_score_deltas: Some([6, -2, -2, -2]),
            },
        },
        PlayerEvent {
            seq: 14,
            kind: PlayerEventKind::HandSettled {
                settlement: win_settlement,
            },
        },
        PlayerEvent {
            seq: 15,
            kind: PlayerEventKind::NextHandConfirmed { seat: Seat::West },
        },
        PlayerEvent {
            seq: 16,
            kind: PlayerEventKind::MatchFinished { summary },
        },
    ];
    for ev in &synthetic_events {
        let back: PlayerEvent = serde_json::from_str(&serde_json::to_string(ev).unwrap()).unwrap();
        assert_eq!(&back, ev);
    }

    // Play a live Match to completion and verify projected views roundtrip.
    let wall = build_preset_wall(
        &[
            t(0),
            t(1),
            t(2),
            t(3),
            t(4),
            t(5),
            t(15),
            t(16),
            t(17),
            t(19),
            t(20),
            t(21),
            t(31),
            t(31),
        ],
        &[t(6); 4]
            .into_iter()
            .chain([t(7); 4])
            .chain([t(8); 4])
            .chain([t(9)])
            .collect::<Vec<_>>(),
        &[t(10); 4]
            .into_iter()
            .chain([t(11); 4])
            .chain([t(12); 4])
            .chain([t(13)])
            .collect::<Vec<_>>(),
        &[t(22); 4]
            .into_iter()
            .chain([t(23); 4])
            .chain([t(24); 4])
            .chain([t(25)])
            .collect::<Vec<_>>(),
        &[],
        &[],
    )
    .unwrap();
    let mut preset_match = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();
    preset_match
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    preset_match
        .apply_action(Seat::East, PlayerAction::EndMatch)
        .unwrap();
    let finished_view = project_view(&preset_match, Seat::East);
    assert_eq!(
        serde_json::from_str::<PlayerView>(&serde_json::to_string(&finished_view).unwrap())
            .unwrap(),
        finished_view
    );
    let playing_view = project_view(&game, Seat::East);
    assert_eq!(
        serde_json::from_str::<PlayerView>(&serde_json::to_string(&playing_view).unwrap()).unwrap(),
        playing_view
    );
}
