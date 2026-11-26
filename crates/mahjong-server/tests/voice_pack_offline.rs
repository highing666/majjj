mod common;

use common::VirtualClient;
use mahjong_ai::AiDifficulty;
use mahjong_core::{MatchMode, Seat};
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{
    is_valid_human_voice_pack_id, PersonaId, HUMAN_VOICE_PACK_FEMALE, HUMAN_VOICE_PACK_IDS,
    HUMAN_VOICE_PACK_MALE,
};
use mahjong_server::protocol::{
    ClientMessage, SeatInfo, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{TableOptions, TableRuntime};

fn find_seat(seats: &[SeatInfo; 4], seat: Seat) -> &SeatInfo {
    &seats[seat.index()]
}

#[test]
fn persona_and_human_voice_pack_ids_are_canonical_and_distinct() {
    assert_eq!(HUMAN_VOICE_PACK_MALE, "human_male");
    assert_eq!(HUMAN_VOICE_PACK_FEMALE, "human_female");
    assert_eq!(HUMAN_VOICE_PACK_IDS, ["human_male", "human_female"]);

    assert!(is_valid_human_voice_pack_id("human_male"));
    assert!(is_valid_human_voice_pack_id("human_female"));
    assert!(!is_valid_human_voice_pack_id("quick_hothead"));
    assert!(!is_valid_human_voice_pack_id(""));
    assert!(!is_valid_human_voice_pack_id("../escape"));

    assert_eq!(PersonaId::QuickHothead.voice_pack_id(), "quick_hothead");
    assert_eq!(PersonaId::TeaSarcasm.voice_pack_id(), "tea_sarcasm");
    assert_eq!(PersonaId::StubbornBlamer.voice_pack_id(), "stubborn_blamer");
    assert_eq!(
        PersonaId::ChaosInstigator.voice_pack_id(),
        "chaos_instigator"
    );
    assert_eq!(PersonaId::TiltedMaster.voice_pack_id(), "tilted_master");
}

#[tokio::test]
async fn human_seats_stagger_default_voice_packs_sync_switches_and_preserve_across_seat_moves() {
    let runtime = TableRuntime::with_options(
        8789,
        None,
        TableOptions::seeded(42).with_pacing(PacingMode::Immediate),
    );

    let mut p1 = VirtualClient::connect(&runtime, "c1", "小王").await;
    let mut p2 = VirtualClient::connect(&runtime, "c2", "小李").await;

    // Unseated player cannot set voice pack.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatVoicePack {
            voice_pack_id: HUMAN_VOICE_PACK_FEMALE.to_string(),
        },
    )
    .await;
    let unseated_msgs = p1.drain_all();
    assert!(
        unseated_msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::ErrorNotice { .. })),
        "unseated SetSeatVoicePack must emit ErrorNotice: {unseated_msgs:?}"
    );

    // First human joins East -> auto-assigned human_male.
    p1.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let p1_snap = p1.recv_snapshot().await;
    assert_eq!(
        find_seat(&p1_snap.seats, Seat::East)
            .voice_pack_id
            .as_deref(),
        Some("human_male")
    );
    let _ = p2.drain_all();

    // Second human joins West -> auto-staggered to human_female.
    p2.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::West),
        },
    )
    .await;
    let p2_snap = p2.recv_snapshot().await;
    assert_eq!(
        find_seat(&p2_snap.seats, Seat::West)
            .voice_pack_id
            .as_deref(),
        Some("human_female")
    );
    let _ = p1.drain_all();

    // Human 1 switches to human_female -> synced table-wide immediately via LobbyUpdated.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatVoicePack {
            voice_pack_id: "human_female".to_string(),
        },
    )
    .await;
    let p1_updates = p1.drain_all();
    let p2_updates = p2.drain_all();
    for (who, msgs) in [("p1", &p1_updates), ("p2", &p2_updates)] {
        let updated = msgs.iter().find_map(|m| match m {
            ServerMessage::LobbyUpdated { seats, .. } => Some(seats),
            _ => None,
        });
        let seats = updated.unwrap_or_else(|| panic!("{who} must receive LobbyUpdated: {msgs:?}"));
        assert_eq!(
            find_seat(seats, Seat::East).voice_pack_id.as_deref(),
            Some("human_female")
        );
        assert_eq!(
            find_seat(seats, Seat::West).voice_pack_id.as_deref(),
            Some("human_female")
        );
    }

    // Invalid or AI voice_pack_id is rejected without mutating seat state.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatVoicePack {
            voice_pack_id: "quick_hothead".to_string(),
        },
    )
    .await;
    let invalid_msgs = p1.drain_all();
    assert!(
        invalid_msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::ErrorNotice { .. })),
        "AI voice pack ID must be rejected for human seat: {invalid_msgs:?}"
    );

    // Moving lobby seat preserves human's explicitly chosen voice_pack_id.
    p1.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;
    let moved_snap = p1.recv_snapshot().await;
    assert_eq!(find_seat(&moved_snap.seats, Seat::East).voice_pack_id, None);
    assert_eq!(
        find_seat(&moved_snap.seats, Seat::South)
            .voice_pack_id
            .as_deref(),
        Some("human_female")
    );

    // Pre-assigning a persona to an empty seat populates matching AI voice_pack_id.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::North,
            persona_id: Some(PersonaId::TeaSarcasm),
        },
    )
    .await;
    let persona_updates = p1.drain_all();
    let seats_after_persona = persona_updates
        .iter()
        .find_map(|m| match m {
            ServerMessage::LobbyUpdated { seats, .. } => Some(seats),
            _ => None,
        })
        .expect("LobbyUpdated after SetSeatPersona");
    assert_eq!(
        find_seat(seats_after_persona, Seat::North)
            .voice_pack_id
            .as_deref(),
        Some("tea_sarcasm")
    );

    // Clearing persona clears empty seat's voice_pack_id.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::North,
            persona_id: None,
        },
    )
    .await;
    let cleared_updates = p1.drain_all();
    let seats_after_clear = cleared_updates
        .iter()
        .find_map(|m| match m {
            ServerMessage::LobbyUpdated { seats, .. } => Some(seats),
            _ => None,
        })
        .expect("LobbyUpdated after clearing persona");
    assert_eq!(
        find_seat(seats_after_clear, Seat::North).voice_pack_id,
        None
    );
}

#[tokio::test]
async fn start_match_assigns_ai_voice_packs_preserves_humans_and_takeover_updates_voice_pack() {
    let runtime = TableRuntime::with_options(
        8789,
        None,
        TableOptions::seeded(7).with_pacing(PacingMode::Immediate),
    );

    let mut p1 = VirtualClient::connect(&runtime, "c1", "东家").await;
    let mut observer = VirtualClient::connect(&runtime, "c2", "客串").await;

    p1.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = p1.recv_snapshot().await;
    p1.send(
        &runtime,
        &ClientMessage::SetSeatVoicePack {
            voice_pack_id: "human_female".to_string(),
        },
    )
    .await;
    let _ = p1.drain_all();
    let _ = observer.drain_all();

    // Assign explicit persona on South, leave West & North random.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: Some(PersonaId::QuickHothead),
        },
    )
    .await;
    p1.send(
        &runtime,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    let _ = p1.drain_all();
    let _ = observer.drain_all();

    p1.send(&runtime, &ClientMessage::StartMatch).await;
    let match_snap = p1.recv_snapshot().await;
    let _ = observer.recv_snapshot().await;
    assert_eq!(match_snap.phase, TablePhase::InMatch);

    // Every AI seat has voice_pack_id == persona_id.voice_pack_id(); human preserves human_female.
    let mut ai_seats = Vec::new();
    for info in &match_snap.seats {
        match info.kind {
            SeatKind::Human => {
                assert_eq!(info.voice_pack_id.as_deref(), Some("human_female"));
            }
            SeatKind::Ai => {
                let persona = info.persona_id.expect("AI seat must have persona_id");
                assert_eq!(info.voice_pack_id.as_deref(), Some(persona.voice_pack_id()));
                ai_seats.push(info.seat);
            }
            SeatKind::Empty => panic!("no Empty seats in InMatch"),
        }
    }
    assert_eq!(ai_seats.len(), 3);

    // Human can switch voice pack mid-match and it syncs table-wide immediately.
    p1.send(
        &runtime,
        &ClientMessage::SetSeatVoicePack {
            voice_pack_id: "human_male".to_string(),
        },
    )
    .await;
    let obs_msgs = observer.drain_all();
    let seats_after_switch = obs_msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::LobbyUpdated { seats, .. } => Some(seats),
            _ => None,
        })
        .expect("observer must receive LobbyUpdated after mid-match voice pack switch");
    assert_eq!(
        find_seat(seats_after_switch, match_snap.your_seat.unwrap())
            .voice_pack_id
            .as_deref(),
        Some("human_male")
    );

    // Observer takes over an AI seat mid-match -> AI voice_pack_id is replaced with staggered human pack (human_female).
    let target_ai = ai_seats[0];
    let _ = p1.drain_all();
    observer
        .send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat: target_ai,
                confirm: true,
            },
        )
        .await;
    let takeover_snap = observer.recv_snapshot().await;
    assert_eq!(takeover_snap.your_seat, Some(target_ai));
    assert_eq!(
        find_seat(&takeover_snap.seats, target_ai)
            .voice_pack_id
            .as_deref(),
        Some("human_female"),
        "human taking over AI seat must receive staggered human voice pack instead of AI voice pack"
    );
}
