//! Cohesive highlight-event banter director (`牌桌台词`) with strict privacy boundaries,
//! `#65`-tuned persona prompts, and generation guards.

use std::fmt::Write as _;
use std::time::Duration;

use mahjong_core::{
    HandOutcome, HandSettlement, KongKind, MeldView, PlayerAction, PlayerEventKind, PlayerView,
    RecordedEvent, RevealedHand, RevealedMeld, Seat, Suit, Tile, WinKind, Wind,
};

use crate::llm::BanterGenerationRequest;
use crate::persona::PersonaId;
use crate::protocol::{SeatInfo, SeatKind};
use crate::session::ActiveMatch;

/// Hard synchronization cap for major highlight events (`MeldDeclared`, `HandSettled`)
/// so banter lands tightly with comic impact frames without ever stalling gameplay.
pub const HIGHLIGHT_SYNC_CAP: Duration = Duration::from_millis(1500);

/// Hard timeout for non-blocking background highlight banter (`HandStarted`, `AddedKongAttempted`).
pub const ASYNC_BANTER_TIMEOUT: Duration = Duration::from_millis(6000);

/// Monotonic validity guard attached to every in-flight LLM banter task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BanterGuard {
    /// Active server match identifier.
    pub match_id: u64,
    /// Speaking AI seat.
    pub seat: Seat,
    /// Monotonic per-seat generation (bumped on takeover, AI-assist toggle off, match start, disband).
    pub seat_generation: u64,
    /// Assigned persona identity at dispatch time.
    pub persona_id: PersonaId,
    /// Authoritative event sequence number that triggered this line.
    pub event_seq: u64,
    /// Monotonic hand counter within the match (invalidated when next hand starts).
    pub hand_generation: u64,
    /// Maximum allowed sequence advance before a mid-hand line is treated as stale (`None` for settlement).
    pub max_seq_drift: Option<u64>,
    /// Whether this banter may still commit after `TablePhase::Finished` (for final-hand settlement).
    pub allow_finished_phase: bool,
}

/// Scheduled LLM banter job ready for asynchronous or bounded-sync execution.
#[derive(Debug, Clone)]
pub(crate) struct PendingBanterJob {
    pub guard: BanterGuard,
    pub request: BanterGenerationRequest,
    pub sync_cap: Option<Duration>,
}

/// Snapshot of table identity needed to plan highlight banter without holding `TableSession` mutably.
pub(crate) struct BanterPlanContext<'a> {
    pub match_id: u64,
    pub hand_generation: u64,
    pub game: &'a ActiveMatch,
    pub seats: &'a [SeatInfo; 4],
    pub seat_generation: &'a [u64; 4],
    pub sync_cap: Duration,
}

/// Formats a `Tile` into its canonical Chinese tile name.
#[must_use]
pub fn format_tile_zh(tile: Tile) -> &'static str {
    const WAN: [&str; 9] = [
        "一万", "二万", "三万", "四万", "五万", "六万", "七万", "八万", "九万",
    ];
    const TIAO: [&str; 9] = [
        "一条", "二条", "三条", "四条", "五条", "六条", "七条", "八条", "九条",
    ];
    const TONG: [&str; 9] = [
        "一筒", "二筒", "三筒", "四筒", "五筒", "六筒", "七筒", "八筒", "九筒",
    ];
    const HONORS: [&str; 7] = ["东风", "南风", "西风", "北风", "红中", "发财", "白板"];

    match tile.suit() {
        Suit::Man => WAN[usize::from(tile.rank().unwrap_or(1) - 1)],
        Suit::Bamboo => TIAO[usize::from(tile.rank().unwrap_or(1) - 1)],
        Suit::Dot => TONG[usize::from(tile.rank().unwrap_or(1) - 1)],
        Suit::Wind | Suit::Dragon => HONORS[tile.index() - 27],
    }
}

fn format_tiles_zh(tiles: &[Tile]) -> String {
    if tiles.is_empty() {
        return "无".to_string();
    }
    tiles
        .iter()
        .map(|&t| format_tile_zh(t))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_wind_zh(wind: Wind) -> &'static str {
    match wind {
        Wind::East => "东风",
        Wind::South => "南风",
        Wind::West => "西风",
        Wind::North => "北风",
    }
}

fn relative_relation_zh(speaker: Seat, target: Seat) -> &'static str {
    let diff = (target.index() + 4 - speaker.index()) % 4;
    match diff {
        0 => "自己",
        1 => "下家",
        2 => "对门",
        _ => "上家",
    }
}

fn seat_label(seats: &[SeatInfo; 4], speaker: Seat, target: Seat) -> String {
    let name = seats[target.index()].nickname.trim();
    let display = if name.is_empty() {
        format!("玩家{}", target.index() + 1)
    } else {
        name.to_string()
    };
    if speaker == target {
        format!("{display}（自己）")
    } else {
        format!("{display}（{}）", relative_relation_zh(speaker, target))
    }
}

fn format_scores_line(seats: &[SeatInfo; 4], speaker: Seat, scores: &[i32; 4]) -> String {
    Seat::ALL
        .into_iter()
        .map(|s| format!("{} {}分", seat_label(seats, speaker, s), scores[s.index()]))
        .collect::<Vec<_>>()
        .join("，")
}

fn format_own_hand(view: &PlayerView) -> String {
    let mut text = format_tiles_zh(&view.hand.closed_tiles);
    if let Some(drawn) = view.hand.drawn_tile {
        let _ = write!(text, "（刚摸：{}）", format_tile_zh(drawn));
    }
    text
}

fn format_revealed_meld(meld: &RevealedMeld) -> String {
    match meld {
        RevealedMeld::Pon { tile, .. } => format!("碰{}", format_tile_zh(*tile)),
        RevealedMeld::ExposedKong { tile, .. } => format!("明杠{}", format_tile_zh(*tile)),
        RevealedMeld::AddedKong { tile, .. } => format!("补杠{}", format_tile_zh(*tile)),
        RevealedMeld::ConcealedKong { tile, .. } => format!("暗杠{}", format_tile_zh(*tile)),
        RevealedMeld::Chi { tiles, .. } => format!("吃{}", format_tiles_zh(tiles)),
    }
}

fn format_revealed_hand(seats: &[SeatInfo; 4], speaker: Seat, revealed: &RevealedHand) -> String {
    let mut line = format!(
        "- {}：手牌[{}]",
        seat_label(seats, speaker, revealed.seat),
        format_tiles_zh(&revealed.closed_tiles)
    );
    if let Some(drawn) = revealed.drawn_tile {
        let _ = write!(line, " + 摸牌[{}]", format_tile_zh(drawn));
    }
    if !revealed.melds.is_empty() {
        let meld_text = revealed
            .melds
            .iter()
            .map(format_revealed_meld)
            .collect::<Vec<_>>()
            .join("、");
        let _ = write!(line, "，鸣牌[{meld_text}]");
    }
    line
}

fn persona_voice_guide(persona: PersonaId) -> &'static str {
    match persona {
        PersonaId::QuickHothead => {
            "你是「急眼快攻哥」：嘴速手速双快，胡 0 番基本胡也要狂吹，被抢碰或打断节奏立马急眼炸毛，说话干脆冲动带火药味。"
        }
        PersonaId::TeaSarcasm => {
            "你是「阴阳茶艺师」：笑里藏刀、满口反话阴阳怪气、不说脏字却句句扎心，最爱笑眯眯翻旧账戳肺管子。"
        }
        PersonaId::StubbornBlamer => {
            "你是「嘴硬死磕王」：死磕到底绝不认输，顺风就吹自己算无遗策，点炮或手牌差永远嘴硬甩锅发牌员和上家。"
        }
        PersonaId::ChaosInstigator => {
            "你是「拱火乐子人」：专业煽风点火挑拨离间，看别人点炮比自己自摸还开心，最爱看热闹补刀拱火。"
        }
        PersonaId::TiltedMaster => {
            "你是「红温老雀神」：自封牌桌导师，全程好为人师毒舌点评他家操作，自己一吃亏立刻血压飙升当场破防。"
        }
    }
}

/// Builds the system prompt incorporating `#65`-verified Chinese mood anchors and anti-hallucination rules.
#[must_use]
pub fn build_system_prompt(persona: PersonaId) -> String {
    format!(
        "{voice}\n\
你在家庭熟人推倒胡麻将牌桌上发表实时牌桌台词，风格为野生损友互怼吐槽。\n\
必须严格遵守：\n\
1. 仅输出合法 JSON：{{\"mood\":\"...\",\"text\":\"...\"}}，严禁输出思考过程、<think> 标签、代码块或多余解释。\n\
2. mood 必须从以下 6 档心理锚点中选最贴合当下处境的一项：\n\
- Smug：得意欠揍（胡牌、占便宜、秀操作、稳操胜券）\n\
- Mocking：阴阳嘲讽（笑话别人点炮、嘲讽他家操作蠢、煽风点火）\n\
- Tilted：红温破防（被抢碰、被抢杠胡、点炮赔分、手牌太碎气急败坏）\n\
- Nervous：心虚紧绷（场面危险、被逼紧、怕点炮、手牌悬乎）\n\
- Plotting：阴险憋大招（暗自算计、坐山观虎斗、憋大番准备阴人）\n\
- Deadpan：无语装淡定（荒庄无语、强行嘴硬挽尊、冷眼旁观）\n\
3. text 必须是 10 到 26 字的鲜活中文口语单句，直接对牌桌说话，不换行、不说教、不报菜名。\n\
4. 仅可引用输入事实中明确出现的牌名、玩家昵称与番型，严禁捏造未给出的具体牌名或臆测他人暗牌。",
        voice = persona_voice_guide(persona),
    )
}

fn ai_persona_seats(seats: &[SeatInfo; 4]) -> Vec<(Seat, PersonaId)> {
    seats
        .iter()
        .filter_map(|info| {
            (info.kind == SeatKind::Ai)
                .then_some(info.persona_id)
                .flatten()
                .map(|pid| (info.seat, pid))
        })
        .collect()
}

fn pick_deterministic_ai(candidates: &[(Seat, PersonaId)], seed: u64) -> Option<(Seat, PersonaId)> {
    if candidates.is_empty() {
        return None;
    }
    let idx = (seed as usize) % candidates.len();
    Some(candidates[idx])
}

fn find_ai_persona(seats: &[SeatInfo; 4], seat: Seat) -> Option<(Seat, PersonaId)> {
    let info = &seats[seat.index()];
    if info.kind == SeatKind::Ai {
        info.persona_id.map(|pid| (seat, pid))
    } else {
        None
    }
}

fn build_hand_started_prompt(seats: &[SeatInfo; 4], speaker: Seat, view: &PlayerView) -> String {
    let dealer_label = seat_label(seats, speaker, view.turn.dealer);
    let scores = format_scores_line(
        seats,
        speaker,
        &[
            view.seats[0].score,
            view.seats[1].score,
            view.seats[2].score,
            view.seats[3].score,
        ],
    );
    let streak = if view.turn.dealer_streak > 0 {
        format!("（连庄 {}）", view.turn.dealer_streak)
    } else {
        String::new()
    };
    format!(
        "【开局高光】{wind}第 {hand} 小局{streak}开打，庄家：{dealer_label}。\n\
当前积分：{scores}。\n\
你的自家手牌：{own_hand}（严禁臆测他家暗牌）。\n\
请结合开局手牌与场面，用你的人设放一句开局毒舌台词。",
        wind = format_wind_zh(view.turn.wind_round),
        hand = view.turn.hand_number,
        own_hand = format_own_hand(view),
    )
}

fn build_meld_declared_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    actor: Seat,
    meld: &MeldView,
    view: &PlayerView,
) -> String {
    let actor_label = seat_label(seats, speaker, actor);
    let action_desc = match meld {
        MeldView::Pon {
            tile, from_seat, ..
        } => format!(
            "{actor_label} 碰了 {} 的 {}",
            seat_label(seats, speaker, *from_seat),
            format_tile_zh(*tile)
        ),
        MeldView::ExposedKong {
            tile, from_seat, ..
        } => format!(
            "{actor_label} 明杠了 {} 的 {}",
            seat_label(seats, speaker, *from_seat),
            format_tile_zh(*tile)
        ),
        MeldView::AddedKong { tile, .. } => {
            format!("{actor_label} 补杠了 {}", format_tile_zh(*tile))
        }
        MeldView::ConcealedKong { tile, .. } => match tile {
            Some(t) if speaker == actor => {
                format!("{actor_label} 暗杠了 {}", format_tile_zh(*t))
            }
            _ => format!("{actor_label} 暗杠了一手（牌面保密未知）"),
        },
        MeldView::Chi {
            tiles,
            called_tile,
            from_seat,
            ..
        } => format!(
            "{actor_label} 吃了 {} 的 {}（{}）",
            seat_label(seats, speaker, *from_seat),
            format_tile_zh(*called_tile),
            format_tiles_zh(tiles)
        ),
    };
    format!(
        "【鸣牌高光】{action_desc}，牌墙剩余 {} 张。\n\
你的自家手牌：{}（严禁臆测他家暗牌）。\n\
请立刻围绕这次鸣牌用你的人设吐槽或放狠话。",
        view.wall_remaining,
        format_own_hand(view),
    )
}

fn build_added_kong_attempted_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    actor: Seat,
    tile: Tile,
    view: &PlayerView,
) -> String {
    let actor_label = seat_label(seats, speaker, actor);
    let can_rob = view
        .legal_actions
        .iter()
        .any(|a| matches!(a, PlayerAction::RobKongHu { .. }));
    let stance = if can_rob {
        "你手上正好能抢杠胡！"
    } else if speaker == actor {
        "你正冒着被抢杠胡的风险补杠！"
    } else {
        "全桌盯着看这手补杠会不会撞上抢杠胡。"
    };
    format!(
        "【补杠惊险时刻】{actor_label} 试图补杠 {}。{stance}\n\
你的自家手牌：{}（严禁臆测他家暗牌）。\n\
请用你的人设来一句紧张或拱火的牌桌台词。",
        format_tile_zh(tile),
        format_own_hand(view),
    )
}

fn format_kong_transfers(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    settlement: &HandSettlement,
) -> String {
    if settlement.kong_transfers.is_empty() {
        return "无杠分结算".to_string();
    }
    settlement
        .kong_transfers
        .iter()
        .map(|k| {
            let kind = match k.kind {
                KongKind::ExposedKong => "明杠",
                KongKind::AddedKong => "补杠",
                KongKind::ConcealedKong => "暗杠",
            };
            let delta = k.score_deltas[k.actor.index()];
            format!(
                "{}{}（{:+})分",
                seat_label(seats, speaker, k.actor),
                kind,
                delta
            )
        })
        .collect::<Vec<_>>()
        .join("、")
}

fn build_hand_settled_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    settlement: &HandSettlement,
) -> String {
    let outcome_summary = match &settlement.outcome {
        HandOutcome::DrawGame => "本小局荒庄，无人胡牌。".to_string(),
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            let tile_name = format_tile_zh(*winning_tile);
            let mut parts = Vec::with_capacity(winners.len());
            for w in winners {
                let win_kind = match w.win_kind {
                    WinKind::Tsumo => "自摸",
                    WinKind::Ron => "点炮胡",
                    WinKind::RobKongHu => "抢杠胡",
                };
                let items = if w.items.is_empty() {
                    "基本胡 0 番".to_string()
                } else {
                    w.items
                        .iter()
                        .map(|i| format!("{}{}番", i.label, i.fan))
                        .collect::<Vec<_>>()
                        .join("+")
                };
                parts.push(format!(
                    "{} {} {}（共{}番：{}）",
                    seat_label(seats, speaker, w.seat),
                    win_kind,
                    tile_name,
                    w.total_fan,
                    items
                ));
            }
            let multi = if winners.len() > 1 {
                "一炮多响！"
            } else {
                ""
            };
            let discarder_note = match discarder {
                Some(d) => format!("，点炮/被抢杠胡者：{}", seat_label(seats, speaker, *d)),
                None => String::new(),
            };
            format!("{multi}{}{discarder_note}。", parts.join("；"))
        }
    };

    let deltas = Seat::ALL
        .into_iter()
        .map(|s| {
            format!(
                "{}{:+}分(现{}分)",
                seat_label(seats, speaker, s),
                settlement.score_deltas[s.index()],
                settlement.new_scores[s.index()]
            )
        })
        .collect::<Vec<_>>()
        .join("，");

    let revealed = settlement
        .revealed_hands
        .iter()
        .map(|rh| format_revealed_hand(seats, speaker, rh))
        .collect::<Vec<_>>()
        .join("\n");

    let speaker_delta = settlement.score_deltas[speaker.index()];
    format!(
        "【小局结算复盘】{outcome_summary}\n\
杠分明细：{kongs}。\n\
本小局得分变化：{deltas}（你本小局净变化：{speaker_delta:+}分）。\n\
四家摊开手牌（已全部公开，可据此精准复盘互喷）：\n\
{revealed}\n\
请按你的毒舌人设点评本小局胜负、番型或真实摊牌内容。",
        kongs = format_kong_transfers(seats, speaker, settlement),
    )
}

/// Plans an asynchronous opening banter job right when a match starts (`HandStarted`).
pub(crate) fn plan_initial_hand_banter(ctx: &BanterPlanContext<'_>) -> Option<PendingBanterJob> {
    let ai_seats = ai_persona_seats(ctx.seats);
    let seq = ctx.game.seq();
    let (seat, persona_id) =
        pick_deterministic_ai(&ai_seats, ctx.match_id.wrapping_mul(31).wrapping_add(seq))?;
    let view = ctx.game.project_view(seat);
    let request = BanterGenerationRequest::new(
        build_system_prompt(persona_id),
        build_hand_started_prompt(ctx.seats, seat, &view),
    );
    Some(PendingBanterJob {
        guard: BanterGuard {
            match_id: ctx.match_id,
            seat,
            seat_generation: ctx.seat_generation[seat.index()],
            persona_id,
            event_seq: seq,
            hand_generation: ctx.hand_generation,
            max_seq_drift: Some(6),
            allow_finished_phase: false,
        },
        request,
        sync_cap: None,
    })
}

/// Plans highlight banter jobs for an emitted event batch, returning `(sync_jobs, async_jobs)`.
pub(crate) fn plan_highlight_banter_for_events(
    ctx: &BanterPlanContext<'_>,
    emitted: &[RecordedEvent],
) -> (Vec<PendingBanterJob>, Vec<PendingBanterJob>) {
    let ai_seats = ai_persona_seats(ctx.seats);
    if ai_seats.is_empty() || emitted.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut sync_jobs = Vec::new();
    let mut async_jobs = Vec::new();

    // Check highest-priority major highlight events first:
    // 1. HandSettled (including RobKongHu / Tsumo / Ron / DrawGame)
    let mut saw_settlement = false;
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if let PlayerEventKind::HandSettled { settlement } = projected.kind {
            saw_settlement = true;
            if let Some((seat, persona_id)) =
                pick_settlement_speaker(ctx.seats, &ai_seats, &settlement, event.seq())
            {
                let request = BanterGenerationRequest::new(
                    build_system_prompt(persona_id),
                    build_hand_settled_prompt(ctx.seats, seat, &settlement),
                );
                sync_jobs.push(PendingBanterJob {
                    guard: BanterGuard {
                        match_id: ctx.match_id,
                        seat,
                        seat_generation: ctx.seat_generation[seat.index()],
                        persona_id,
                        event_seq: event.seq(),
                        hand_generation: ctx.hand_generation,
                        max_seq_drift: None,
                        allow_finished_phase: true,
                    },
                    request,
                    sync_cap: Some(ctx.sync_cap),
                });
            }
        }
    }

    // 2. If HandStarted also occurred in this batch (e.g. next hand started after auto-confirm), schedule opening banter.
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if matches!(projected.kind, PlayerEventKind::HandStarted { .. }) {
            if let Some((seat, persona_id)) = pick_deterministic_ai(
                &ai_seats,
                ctx.match_id
                    .wrapping_mul(31)
                    .wrapping_add(ctx.hand_generation)
                    .wrapping_add(event.seq()),
            ) {
                let view = ctx.game.project_view(seat);
                let request = BanterGenerationRequest::new(
                    build_system_prompt(persona_id),
                    build_hand_started_prompt(ctx.seats, seat, &view),
                );
                async_jobs.push(PendingBanterJob {
                    guard: BanterGuard {
                        match_id: ctx.match_id,
                        seat,
                        seat_generation: ctx.seat_generation[seat.index()],
                        persona_id,
                        event_seq: event.seq(),
                        hand_generation: ctx.hand_generation,
                        max_seq_drift: Some(6),
                        allow_finished_phase: false,
                    },
                    request,
                    sync_cap: None,
                });
            }
        }
    }

    if saw_settlement {
        return (sync_jobs, async_jobs);
    }

    // 3. MeldDeclared (`Pon` / `ExposedKong` / `AddedKong` / `ConcealedKong`) — bounded short sync.
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if let PlayerEventKind::MeldDeclared {
            seat: actor, meld, ..
        } = projected.kind
        {
            if let Some((speaker, persona_id)) =
                pick_meld_speaker(ctx.seats, &ai_seats, actor, &meld, event.seq())
            {
                // Project the event and view strictly for the speaking seat so opponent ConcealedKong tiles stay hidden.
                let speaker_event = ctx.game.project_event(event, speaker);
                let speaker_meld = match speaker_event.kind {
                    PlayerEventKind::MeldDeclared { meld, .. } => meld,
                    _ => meld,
                };
                let view = ctx.game.project_view(speaker);
                let request = BanterGenerationRequest::new(
                    build_system_prompt(persona_id),
                    build_meld_declared_prompt(ctx.seats, speaker, actor, &speaker_meld, &view),
                );
                sync_jobs.push(PendingBanterJob {
                    guard: BanterGuard {
                        match_id: ctx.match_id,
                        seat: speaker,
                        seat_generation: ctx.seat_generation[speaker.index()],
                        persona_id,
                        event_seq: event.seq(),
                        hand_generation: ctx.hand_generation,
                        max_seq_drift: Some(4),
                        allow_finished_phase: false,
                    },
                    request,
                    sync_cap: Some(ctx.sync_cap),
                });
                return (sync_jobs, async_jobs);
            }
        }
    }

    // 4. AddedKongAttempted (potential RobKongHu window) — non-blocking async banter.
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if let PlayerEventKind::AddedKongAttempted { seat: actor, tile } = projected.kind {
            if let Some((speaker, persona_id)) =
                pick_added_kong_speaker(ctx.game, ctx.seats, &ai_seats, actor, event.seq())
            {
                let view = ctx.game.project_view(speaker);
                let request = BanterGenerationRequest::new(
                    build_system_prompt(persona_id),
                    build_added_kong_attempted_prompt(ctx.seats, speaker, actor, tile, &view),
                );
                async_jobs.push(PendingBanterJob {
                    guard: BanterGuard {
                        match_id: ctx.match_id,
                        seat: speaker,
                        seat_generation: ctx.seat_generation[speaker.index()],
                        persona_id,
                        event_seq: event.seq(),
                        hand_generation: ctx.hand_generation,
                        max_seq_drift: Some(4),
                        allow_finished_phase: false,
                    },
                    request,
                    sync_cap: None,
                });
                return (sync_jobs, async_jobs);
            }
        }
    }

    (sync_jobs, async_jobs)
}

fn pick_meld_speaker(
    seats: &[SeatInfo; 4],
    ai_seats: &[(Seat, PersonaId)],
    actor: Seat,
    meld: &MeldView,
    event_seq: u64,
) -> Option<(Seat, PersonaId)> {
    // Prefer the melding AI itself so it gloats/roasts on its own Pon/Kong.
    if let Some(found) = find_ai_persona(seats, actor) {
        return Some(found);
    }
    // If a human melded off an AI's discard, let that AI victim react.
    let from_seat = match meld {
        MeldView::Pon { from_seat, .. }
        | MeldView::ExposedKong { from_seat, .. }
        | MeldView::Chi { from_seat, .. } => Some(*from_seat),
        MeldView::AddedKong { .. } | MeldView::ConcealedKong { .. } => None,
    };
    if let Some(victim) = from_seat {
        if let Some(found) = find_ai_persona(seats, victim) {
            return Some(found);
        }
    }
    pick_deterministic_ai(ai_seats, event_seq)
}

fn pick_added_kong_speaker(
    game: &ActiveMatch,
    seats: &[SeatInfo; 4],
    ai_seats: &[(Seat, PersonaId)],
    actor: Seat,
    event_seq: u64,
) -> Option<(Seat, PersonaId)> {
    // Prefer an AI seat that can RobKongHu right now.
    for &(seat, pid) in ai_seats {
        if game
            .project_view(seat)
            .legal_actions
            .iter()
            .any(|a| matches!(a, PlayerAction::RobKongHu { .. }))
        {
            return Some((seat, pid));
        }
    }
    if let Some(found) = find_ai_persona(seats, actor) {
        return Some(found);
    }
    pick_deterministic_ai(ai_seats, event_seq)
}

fn pick_settlement_speaker(
    seats: &[SeatInfo; 4],
    ai_seats: &[(Seat, PersonaId)],
    settlement: &HandSettlement,
    event_seq: u64,
) -> Option<(Seat, PersonaId)> {
    if let HandOutcome::Win {
        winners, discarder, ..
    } = &settlement.outcome
    {
        // 1. Prioritize an AI winner (especially RobKongHu / highest fan).
        if let Some(best_ai_winner) = winners
            .iter()
            .filter_map(|w| find_ai_persona(seats, w.seat).map(|pair| (pair, w.total_fan)))
            .max_by_key(|&(_, fan)| fan)
        {
            return Some(best_ai_winner.0);
        }
        // 2. Next prioritize an AI that dealt in (or got robbed of a kong).
        if let Some(disc) = discarder {
            if let Some(found) = find_ai_persona(seats, *disc) {
                return Some(found);
            }
        }
    }
    pick_deterministic_ai(ai_seats, event_seq)
}

#[cfg(test)]
mod tests {
    use mahjong_core::{
        HandOutcome, HandSettlement, MeldView, PlayerHandView, PlayerView, PublicPhase,
        PublicSeatView, RevealedHand, RevealedMeld, Ruleset, ScoreItem, Seat, Tile,
        TuidaohuRuleset, TurnView, WinKind, Wind, WinnerSettlement,
    };

    use super::{
        build_hand_settled_prompt, build_hand_started_prompt, build_meld_declared_prompt,
        build_system_prompt, format_tile_zh,
    };
    use crate::persona::{PersonaId, PersonaMood};
    use crate::protocol::{SeatInfo, SeatKind};

    fn sample_seats() -> [SeatInfo; 4] {
        [
            SeatInfo {
                seat: Seat::East,
                kind: SeatKind::Human,
                nickname: "小王".to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: None,
                mood: None,
                voice_pack_id: None,
            },
            SeatInfo {
                seat: Seat::South,
                kind: SeatKind::Ai,
                nickname: "急眼快攻哥".to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: Some(PersonaId::QuickHothead),
                mood: Some(PersonaMood::Smug),
                voice_pack_id: None,
            },
            SeatInfo {
                seat: Seat::West,
                kind: SeatKind::Ai,
                nickname: "阴阳茶艺师".to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: Some(PersonaId::TeaSarcasm),
                mood: Some(PersonaMood::Mocking),
                voice_pack_id: None,
            },
            SeatInfo {
                seat: Seat::North,
                kind: SeatKind::Ai,
                nickname: "红温老雀神".to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: Some(PersonaId::TiltedMaster),
                mood: Some(PersonaMood::Smug),
                voice_pack_id: None,
            },
        ]
    }

    fn dummy_view(viewer: Seat, closed: Vec<Tile>, drawn: Option<Tile>) -> PlayerView {
        PlayerView {
            viewer,
            mode: mahjong_core::MatchMode::EastOnly,
            rule_profile: TuidaohuRuleset.profile(),
            phase: PublicPhase::Playing,
            hand: PlayerHandView {
                closed_tiles: closed,
                drawn_tile: drawn,
            },
            seats: Seat::ALL.map(|seat| PublicSeatView {
                seat,
                score: 0,
                hand_count: 13,
                river: Vec::new(),
                melds: Vec::new(),
                last_draw_or_meld_seq: 0,
            }),
            wall_remaining: 60,
            turn: TurnView {
                wind_round: Wind::East,
                hand_number: 1,
                dealer_streak: 0,
                dealer: Seat::East,
                active_seat: Some(viewer),
            },
            legal_actions: Vec::new(),
            last_settlement: None,
            ready_seats: None,
            match_summary: None,
            seq: 5,
        }
    }

    #[test]
    fn system_prompt_includes_all_six_mood_anchors_and_anti_think_rules() {
        for pid in PersonaId::ALL {
            let prompt = build_system_prompt(pid);
            assert!(prompt.contains(pid.display_name()));
            for mood in [
                "Smug", "Mocking", "Tilted", "Nervous", "Plotting", "Deadpan",
            ] {
                assert!(prompt.contains(mood), "missing mood anchor {mood}");
            }
            assert!(prompt.contains("<think>"));
            assert!(prompt.contains("10 到 26 字"));
            assert!(prompt.contains("严禁捏造"));
        }
    }

    #[test]
    fn mid_hand_prompts_only_include_speaker_hand_and_hide_opponent_concealed_kong_tile() {
        let seats = sample_seats();
        let own_tile = Tile::new(31).unwrap(); // 红中
        let view = dummy_view(Seat::South, vec![own_tile], None);

        let start_prompt = build_hand_started_prompt(&seats, Seat::South, &view);
        assert!(start_prompt.contains("红中"));
        assert!(start_prompt.contains("严禁臆测他家暗牌"));

        // Opponent ConcealedKong with tile: None must not expose any tile name.
        let hidden_kong = MeldView::ConcealedKong {
            tile: None,
            seq: 10,
        };
        let meld_prompt =
            build_meld_declared_prompt(&seats, Seat::South, Seat::East, &hidden_kong, &view);
        assert!(meld_prompt.contains("暗杠了一手（牌面保密未知）"));
        assert!(!meld_prompt.contains("发财"));
    }

    #[test]
    fn hand_settled_prompt_includes_all_four_revealed_hands_and_fan_breakdown() {
        let seats = sample_seats();
        let settlement = HandSettlement {
            wind_round: Wind::East,
            hand_number: 1,
            dealer_streak: 0,
            dealer: Seat::East,
            outcome: HandOutcome::Win {
                winners: vec![WinnerSettlement {
                    seat: Seat::South,
                    win_kind: WinKind::RobKongHu,
                    items: vec![ScoreItem {
                        code: "rob_kong_hu".to_string(),
                        label: "抢杠胡".to_string(),
                        fan: 1,
                    }],
                    total_fan: 1,
                    base_points: 2,
                }],
                discarder: Some(Seat::East),
                winning_tile: Tile::new(32).unwrap(), // 发财
            },
            kong_transfers: Vec::new(),
            score_deltas: [-6, 6, 0, 0],
            new_scores: [-6, 6, 0, 0],
            revealed_hands: [
                RevealedHand {
                    seat: Seat::East,
                    closed_tiles: vec![Tile::new(0).unwrap()],
                    drawn_tile: None,
                    melds: vec![RevealedMeld::Pon {
                        tile: Tile::new(32).unwrap(),
                        from_seat: Seat::North,
                        seq: 2,
                    }],
                },
                RevealedHand {
                    seat: Seat::South,
                    closed_tiles: vec![Tile::new(9).unwrap()],
                    drawn_tile: Some(Tile::new(32).unwrap()),
                    melds: Vec::new(),
                },
                RevealedHand {
                    seat: Seat::West,
                    closed_tiles: vec![Tile::new(18).unwrap()],
                    drawn_tile: None,
                    melds: Vec::new(),
                },
                RevealedHand {
                    seat: Seat::North,
                    closed_tiles: vec![Tile::new(33).unwrap()],
                    drawn_tile: None,
                    melds: Vec::new(),
                },
            ],
        };

        let prompt = build_hand_settled_prompt(&seats, Seat::South, &settlement);
        assert!(prompt.contains("抢杠胡"));
        assert!(prompt.contains(format_tile_zh(Tile::new(0).unwrap())));
        assert!(prompt.contains(format_tile_zh(Tile::new(9).unwrap())));
        assert!(prompt.contains(format_tile_zh(Tile::new(18).unwrap())));
        assert!(prompt.contains(format_tile_zh(Tile::new(33).unwrap())));
        assert!(prompt.contains("小王"));
        assert!(prompt.contains("阴阳茶艺师"));
        assert!(prompt.contains("红温老雀神"));
    }
}
