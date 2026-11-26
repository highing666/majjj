//! Temporary AI assistance, cross-device offline seat takeover, mid-match AI hot takeover,
//! and per-seat monotonic generation guards against stale Rayon AI completions.

use mahjong_ai::AiStyle;
use mahjong_core::{PublicPhase, Seat};
use uuid::Uuid;

use crate::protocol::{SeatKind, ServerMessage, TablePhase};
use crate::session::{SeatRecord, TableSession};

/// Per-hand per-seat attribution metadata tracked for match history persistence (#51).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct HandAttribution {
    /// `true` if temporary AI assistance was active for that seat at any point during the hand.
    pub assisted_by_ai: [bool; 4],
    /// `true` if an AI seat was taken over by a human during the hand.
    pub took_over_ai_in_hand: [bool; 4],
}

impl HandAttribution {
    /// Initializes attribution at the start of a new hand from the current seat state.
    pub(crate) fn from_seats(seats: &[SeatRecord; 4]) -> Self {
        let mut assisted_by_ai = [false; 4];
        for (idx, slot) in seats.iter().enumerate() {
            assisted_by_ai[idx] = slot.info.kind == SeatKind::Human && slot.info.ai_assisted;
        }
        Self {
            assisted_by_ai,
            took_over_ai_in_hand: [false; 4],
        }
    }

    pub(crate) fn mark_ai_assisted(&mut self, seat: Seat) {
        self.assisted_by_ai[seat.index()] = true;
    }

    pub(crate) fn mark_took_over_ai(&mut self, seat: Seat) {
        self.took_over_ai_in_hand[seat.index()] = true;
    }
}

struct CallerIdentity<'a> {
    conn_id: u64,
    client_id: &'a str,
    seat_token: &'a str,
    nickname: &'a str,
}

fn effective_nickname(raw: &str, seat: Seat) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("玩家{}", seat.index() + 1)
    } else {
        trimmed.to_string()
    }
}

impl TableSession {
    /// Returns `true` when `seat` is currently driven by the AI decision pool
    /// (either an `Ai` seat or a `Human` seat with temporary `ai_assisted` enabled).
    pub(crate) fn is_ai_controlled(&self, seat: Seat) -> bool {
        let info = &self.seats[seat.index()].info;
        info.kind == SeatKind::Ai || (info.kind == SeatKind::Human && info.ai_assisted)
    }

    /// Increments and returns the monotonic generation for `seat`, invalidating any in-flight AI work.
    pub(crate) fn bump_seat_generation(&mut self, seat: Seat) -> u64 {
        let idx = seat.index();
        self.seat_generation[idx] += 1;
        self.seat_generation[idx]
    }

    /// Increments the monotonic generation for all four seats (on match start or disband/reset).
    pub(crate) fn bump_all_seat_generations(&mut self) {
        for seat in Seat::ALL {
            let _ = self.bump_seat_generation(seat);
        }
    }

    /// Returns the current hand's attribution metadata if a match is active.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current_hand_attribution(&self) -> Option<HandAttribution> {
        self.active_match.as_ref().map(|a| a.hand_attribution)
    }

    fn find_seat_conn_id(&self, seat: Seat) -> Option<u64> {
        self.connections
            .iter()
            .find_map(|(&id, conn)| (conn.seat == Some(seat)).then_some(id))
    }

    fn reconcile_pending_destructive_op_on_seat_join(&mut self, seat: Seat) -> bool {
        let Some(op) = self.pending_destructive_op.as_mut() else {
            return false;
        };
        if op.required_seats.contains(&seat) {
            return false;
        }
        op.required_seats.push(seat);
        true
    }

    pub(crate) fn handle_toggle_ai_assist(&mut self, conn_id: u64, seat: Seat, enabled: bool) {
        let Some((caller_seat, tx)) = self.require_seated_human(conn_id, "toggling AI assist")
        else {
            return;
        };
        if self.phase != TablePhase::InMatch {
            Self::send_error(&tx, "AI assist can only be toggled during an active match");
            return;
        }

        let target_info = &self.seats[seat.index()].info;
        if target_info.kind != SeatKind::Human {
            Self::send_error(&tx, "can only toggle AI assist for a human seat");
            return;
        }

        let allowed = if enabled {
            caller_seat == seat || !target_info.connected
        } else {
            caller_seat == seat || !target_info.connected || target_info.ai_assisted
        };
        if !allowed {
            Self::send_error(
                &tx,
                "can only toggle AI assist for your own seat or an offline human seat",
            );
            return;
        }

        if enabled {
            let difficulty = self.config.ai_difficulty;
            let slot = &mut self.seats[seat.index()];
            slot.info.ai_assisted = true;
            slot.info.ai_difficulty = Some(difficulty);
            slot.info.ai_style = Some(AiStyle::Balanced);

            let public_phase = self
                .active_match
                .as_ref()
                .map(|a| a.game.project_view(Seat::East).phase);
            if public_phase == Some(PublicPhase::Playing) {
                if let Some(active) = self.active_match.as_mut() {
                    active.hand_attribution.mark_ai_assisted(seat);
                }
            }

            self.broadcast_lobby_updated(None);

            if public_phase == Some(PublicPhase::BetweenHands)
                && self
                    .active_match
                    .as_ref()
                    .is_some_and(|a| a.pending_release.is_none())
            {
                let mut emitted = Vec::new();
                self.auto_confirm_ai_between_hands(&mut emitted);
                if !emitted.is_empty() {
                    self.finalize_emitted_events(emitted);
                }
            }
            return;
        }

        let _ = self.bump_seat_generation(seat);
        self.in_flight_ai.retain(|k| k.seat != seat);

        let slot = &mut self.seats[seat.index()];
        slot.info.ai_assisted = false;
        slot.info.ai_difficulty = None;
        slot.info.ai_style = None;

        self.broadcast_lobby_updated(None);
        if let Some(target_conn_id) = self.find_seat_conn_id(seat) {
            if let Some(target_conn) = self.connections.get(&target_conn_id) {
                let snapshot = self.build_snapshot(target_conn);
                Self::send_to(&target_conn.tx, ServerMessage::TableSnapshot(snapshot));
            }
        }
    }

    pub(crate) fn handle_take_over_seat(&mut self, conn_id: u64, seat: Seat, confirm: bool) {
        let Some(caller) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        if caller.seat.is_some() {
            Self::send_error(&caller.tx, "already seated; cannot take over another seat");
            return;
        }
        if !confirm {
            Self::send_error(&caller.tx, "TakeOverSeat requires confirm: true");
            return;
        }

        let caller_identity = CallerIdentity {
            conn_id,
            client_id: &caller.client_id,
            seat_token: &caller.seat_token,
            nickname: &caller.nickname,
        };

        match self.seats[seat.index()].info.kind {
            SeatKind::Empty => {
                Self::send_error(
                    &caller.tx,
                    "cannot take over an empty seat; use JoinSeat in Lobby",
                );
            }
            SeatKind::Human if self.seats[seat.index()].info.connected => {
                Self::send_error(&caller.tx, "cannot take over an online human seat");
            }
            SeatKind::Human => {
                self.take_over_offline_human_seat(caller_identity, seat);
            }
            SeatKind::Ai => {
                if self.phase != TablePhase::InMatch {
                    Self::send_error(
                        &caller.tx,
                        "can only take over an AI seat during an active match",
                    );
                    return;
                }
                self.take_over_ai_seat(caller_identity, seat);
            }
        }
    }

    fn take_over_offline_human_seat(&mut self, caller: CallerIdentity<'_>, seat: Seat) {
        if let Some(old_seat_token) = self.seats[seat.index()].seat_token.take() {
            self.issued_tokens.remove(&old_seat_token);
            self.token_to_conn.remove(&old_seat_token);
        }

        self.issued_tokens.remove(caller.seat_token);
        self.token_to_conn.remove(caller.seat_token);

        let new_token = Uuid::new_v4().to_string();
        self.issued_tokens
            .insert(new_token.clone(), caller.client_id.to_string());
        self.token_to_conn.insert(new_token.clone(), caller.conn_id);

        let _ = self.bump_seat_generation(seat);
        self.in_flight_ai.retain(|k| k.seat != seat);

        let slot = &mut self.seats[seat.index()];
        slot.info.connected = true;
        slot.info.ai_assisted = false;
        slot.info.ai_difficulty = None;
        slot.info.ai_style = None;
        if slot.info.nickname.trim().is_empty() {
            slot.info.nickname = effective_nickname(caller.nickname, seat);
        }
        slot.client_id = Some(caller.client_id.to_string());
        slot.seat_token = Some(new_token.clone());

        if let Some(conn_mut) = self.connections.get_mut(&caller.conn_id) {
            conn_mut.seat = Some(seat);
            conn_mut.seat_token = new_token;
        }

        if !caller.nickname.trim().is_empty() {
            self.history.record_nickname(caller.nickname);
        }

        let pending_op_updated = self.reconcile_pending_destructive_op_on_seat_join(seat);
        self.notify_takeover_participants(caller.conn_id, pending_op_updated);
    }

    fn take_over_ai_seat(&mut self, caller: CallerIdentity<'_>, seat: Seat) {
        let _ = self.bump_seat_generation(seat);
        self.in_flight_ai.retain(|k| k.seat != seat);

        if !caller.nickname.trim().is_empty() {
            self.history.record_nickname(caller.nickname);
        }

        let slot = &mut self.seats[seat.index()];
        slot.info.kind = SeatKind::Human;
        slot.info.nickname = effective_nickname(caller.nickname, seat);
        slot.info.connected = true;
        slot.info.ai_assisted = false;
        slot.info.ai_difficulty = None;
        slot.info.ai_style = None;
        slot.client_id = Some(caller.client_id.to_string());
        slot.seat_token = Some(caller.seat_token.to_string());

        if let Some(conn_mut) = self.connections.get_mut(&caller.conn_id) {
            conn_mut.seat = Some(seat);
        }

        if let Some(active) = self.active_match.as_mut() {
            active.hand_attribution.mark_took_over_ai(seat);
        }

        let pending_op_updated = self.reconcile_pending_destructive_op_on_seat_join(seat);
        self.notify_takeover_participants(caller.conn_id, pending_op_updated);
    }

    fn notify_takeover_participants(&self, caller_conn_id: u64, pending_op_updated: bool) {
        if let Some(caller_conn) = self.connections.get(&caller_conn_id) {
            let snapshot = self.build_snapshot(caller_conn);
            Self::send_to(&caller_conn.tx, ServerMessage::TableSnapshot(snapshot));
        }
        self.broadcast_lobby_updated(Some(caller_conn_id));
        if pending_op_updated {
            self.broadcast_snapshots_excluding(Some(caller_conn_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mahjong_core::{
        build_preset_wall, MatchMode, PlayerAction, PresetWallProvider, Seat, Tile,
    };
    use tokio::sync::mpsc;

    use crate::pacing::{AiTurnTrigger, PacingMode};
    use crate::protocol::{LobbyConfig, SeatKind, SeatingRule, TablePace, TablePhase};
    use crate::session::{BoxedWallProvider, OutboundSignal, TableOptions, TableSession};

    fn make_tile(idx: u8) -> Tile {
        Tile::new(idx).expect("valid tile")
    }

    fn tsumo_and_plain_walls() -> TableOptions {
        let mut east_14 = Vec::new();
        for idx in [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 31, 31] {
            east_14.push(make_tile(idx));
        }
        let other_13: Vec<Tile> = [4, 7, 8, 9, 12, 15, 18, 21, 24, 27, 28, 29, 30]
            .into_iter()
            .map(make_tile)
            .collect();
        let wall1 =
            build_preset_wall(&east_14, &other_13, &other_13, &other_13, &[], &[]).expect("wall 1");
        let plain_13: Vec<Tile> = [0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 28, 29, 30]
            .into_iter()
            .map(make_tile)
            .collect();
        let mut plain_14 = plain_13.clone();
        plain_14.push(make_tile(31));
        let wall2 = build_preset_wall(&plain_14, &plain_13, &plain_13, &plain_13, &[], &[])
            .expect("wall 2");
        TableOptions::with_wall_factory(
            77,
            Arc::new(move || {
                BoxedWallProvider(Box::new(PresetWallProvider::new(
                    Seat::East,
                    vec![wall1.clone(), wall2.clone()],
                )))
            }),
        )
        .with_pacing(PacingMode::Humanized)
    }

    fn connect_client(
        session: &mut TableSession,
        client_id: &str,
        nickname: &str,
    ) -> (u64, mpsc::UnboundedReceiver<OutboundSignal>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn_id = session.allocate_conn_id();
        session.handle_hello_for_test(conn_id, client_id, nickname, &tx);
        (conn_id, rx)
    }

    #[tokio::test]
    async fn seat_generation_drops_stale_ai_in_both_delay_planning_and_completion() {
        let mut session = TableSession::with_options(8789, None, tsumo_and_plain_walls());
        let (c1, _rx1) = connect_client(&mut session, "c1", "东家");
        let (c2, _rx2) = connect_client(&mut session, "c2", "观察");

        session.handle_join_seat(c1, Some(Seat::East));
        session.handle_update_lobby_config(
            c1,
            LobbyConfig {
                mode: MatchMode::FreeTable,
                seating: SeatingRule::FixedAdjacent,
                ai_difficulty: mahjong_ai::AiDifficulty::Strong,
                pace: TablePace::Standard,
            },
        );
        session.handle_start_match(c1);

        // Advance East discard so South (AI) receives a pending AI job.
        let seq_before = session.active_match.as_ref().expect("match").game.seq();
        session.handle_submit_action(
            c1,
            seq_before,
            "d1".to_string(),
            PlayerAction::Discard {
                tile: make_tile(31),
                from_drawn: false,
            },
        );
        let release_ticket = session
            .active_match
            .as_ref()
            .and_then(|a| a.pending_release.as_ref())
            .map(|p| p.ticket)
            .expect("humanized discard release ticket");
        let work = session.complete_pending_release(release_ticket);
        assert_eq!(work.ai_jobs.len(), 1);
        let job = &work.ai_jobs[0];
        assert_eq!(job.key.seat, Seat::South);

        // Unseated c2 takes over South AI seat -> increments seat_generation[South].
        session.handle_take_over_seat(c2, Seat::South, true);
        assert_eq!(
            session.seats[Seat::South.index()].info.kind,
            SeatKind::Human
        );

        // Stale Rayon job arriving at plan_ai_action_delay is dropped (returns None).
        let stale_delay = session.plan_ai_action_delay(
            job.key,
            PlayerAction::Discard {
                tile: make_tile(30),
                from_drawn: false,
            },
            AiTurnTrigger::Normal,
            job.step_seed,
        );
        assert_eq!(stale_delay, None);

        // Even if complete_ai_job is reached directly with stale generation, zero state mutation occurs.
        let seq_after_takeover = session.active_match.as_ref().expect("match").game.seq();
        let next = session.complete_ai_job(
            job.key,
            PlayerAction::Discard {
                tile: make_tile(30),
                from_drawn: false,
            },
        );
        assert!(next.is_empty());
        assert_eq!(
            session.active_match.as_ref().expect("match").game.seq(),
            seq_after_takeover
        );
    }

    #[tokio::test]
    async fn hand_attribution_tracks_ai_assist_and_takeover_and_resets_per_hand() {
        let mut session = TableSession::with_options(8789, None, tsumo_and_plain_walls());
        let (c1, _rx1) = connect_client(&mut session, "c1", "东家");
        let (c2, _rx2) = connect_client(&mut session, "c2", "南家");
        let (c3, _rx3) = connect_client(&mut session, "c3", "客串");

        session.handle_join_seat(c1, Some(Seat::East));
        session.handle_join_seat(c2, Some(Seat::South));
        session.handle_update_lobby_config(
            c1,
            LobbyConfig {
                mode: MatchMode::FreeTable,
                seating: SeatingRule::FixedAdjacent,
                ai_difficulty: mahjong_ai::AiDifficulty::Strong,
                pace: TablePace::Standard,
            },
        );
        session.handle_start_match(c1);

        // Enable and then disable AI assist on East during Hand 1 -> assisted_by_ai[East] stays latched!
        session.handle_toggle_ai_assist(c1, Seat::East, true);
        session.handle_toggle_ai_assist(c1, Seat::East, false);

        // Mid-hand AI takeover of West -> took_over_ai_in_hand[West] becomes true.
        session.handle_take_over_seat(c3, Seat::West, true);
        let attr1 = session
            .current_hand_attribution()
            .expect("active hand attribution");
        assert!(attr1.assisted_by_ai[Seat::East.index()]);
        assert!(!attr1.assisted_by_ai[Seat::South.index()]);
        assert!(attr1.took_over_ai_in_hand[Seat::West.index()]);

        // Settle Hand 1 by East Tsumo, confirm all humans -> Hand 2 starts and resets attribution!
        let seq1 = session.active_match.as_ref().expect("match").game.seq();
        session.handle_submit_action(c1, seq1, "win1".to_string(), PlayerAction::Tsumo);
        for (cid, aid) in [(c1, "ok1"), (c2, "ok2"), (c3, "ok3")] {
            let seq_bh = session.active_match.as_ref().expect("match").game.seq();
            session.handle_submit_action(
                cid,
                seq_bh,
                aid.to_string(),
                PlayerAction::ConfirmNextHand,
            );
        }

        let attr2 = session
            .current_hand_attribution()
            .expect("hand 2 attribution");
        assert_eq!(attr2.assisted_by_ai, [false; 4]);
        assert_eq!(attr2.took_over_ai_in_hand, [false; 4]);
    }

    #[test]
    fn offline_human_takeover_also_works_in_lobby() {
        let mut session = TableSession::with_options(8789, None, TableOptions::seeded(1));
        let (c1, _rx1) = connect_client(&mut session, "c1", "原玩家");
        let (c2, _rx2) = connect_client(&mut session, "c2", "新设备");

        session.handle_join_seat(c1, Some(Seat::East));
        session.on_disconnect(c1);
        assert_eq!(session.phase(), TablePhase::Lobby);
        assert!(!session.seats[Seat::East.index()].info.connected);

        session.handle_take_over_seat(c2, Seat::East, true);
        assert!(session.seats[Seat::East.index()].info.connected);
        assert_eq!(
            session.connections.get(&c2).expect("c2").seat,
            Some(Seat::East)
        );
    }
}
