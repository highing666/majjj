//! Cohesive autonomous banter director (`牌桌台词`) covering natural wake-up boundaries,
//! single-in-flight-per-seat deduplication, serial capped AI follow-up chains, and tiered staleness guards.

use std::fmt::Write as _;
use std::time::Duration;

use mahjong_ai::{analyze_hand_facts, HandFacts, HandProgression};
use mahjong_core::{
    HandOutcome, HandSettlement, KongKind, MeldView, PlayerEventKind, PlayerView, RecordedEvent,
    RevealedHand, RevealedMeld, Seat, Suit, Tile, WinKind,
};

use crate::banter_context::{
    current_local_time_zh, seat_direction_and_name, MatchBanterContext, SixBlockInput,
};
use crate::llm::BanterGenerationRequest;
use crate::persona::{PersonaId, PersonaMood};
use crate::persona_memory::{MemoryNoteJob, PersonaMemoryStore};
use crate::protocol::{QuickEmote, SeatInfo, SeatKind, ServerMessage};
use crate::session::ActiveMatch;

/// Hard timeout for non-blocking background banter (aligned with `llm::DEFAULT_REQUEST_TIMEOUT`).
pub const ASYNC_BANTER_TIMEOUT: Duration = Duration::from_secs(20);

/// Natural comedic beat (`~1s`) before a serial autonomous AI follow-up check-in dispatches.
pub const BANTER_CHAIN_REPLY_DELAY: Duration = Duration::from_secs(1);

/// Strict hard upper bound on serial AI-to-AI follow-up rounds per interaction (opener + at most 2 follow-ups).
pub const MAX_BANTER_CHAIN_REPLIES: u8 = 2;

/// Relaxed sequence-drift ceiling for strong-recency reactions within the same hand
/// (human meld/kong reaction, quick emote reply, slow-think nudge, or mid-hand follow-up banter).
pub const STRONG_RECENCY_MAX_SEQ_DRIFT: u64 = 10;

/// Minimum per-seat cooldown between consecutive human `QuickEmote` triggers.
pub const QUICK_EMOTE_COOLDOWN: Duration = Duration::from_secs(4);

/// Waiting threshold before a single playful AI slow-think nudge fires (`15–20s` per Spec #63).
pub const SLOW_THINK_NUDGE_DELAY: Duration = Duration::from_secs(16);

/// Monotonic validity guard attached to every in-flight LLM banter task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BanterGuard {
    /// Monotonic job identifier matching `BanterSessionState::in_flight_seats[seat]` (`0` until claimed).
    pub job_id: u64,
    /// Active server match identifier.
    pub match_id: u64,
    /// Speaking AI seat.
    pub seat: Seat,
    /// Monotonic per-seat generation (bumped on takeover, AI-assist toggle off, match start, disband).
    pub seat_generation: u64,
    /// Assigned persona identity at dispatch time.
    pub persona_id: PersonaId,
    /// Authoritative event sequence number that triggered this wake-up.
    pub event_seq: u64,
    /// Monotonic hand counter within the match (invalidated when next hand starts).
    pub hand_generation: u64,
    /// Maximum allowed sequence advance before a strong-recency line is treated as stale
    /// (`None` for self-check-in / casual chatter and `HandSettled` recaps within the same hand).
    pub max_seq_drift: Option<u64>,
    /// Whether this banter may still commit after `TablePhase::Finished` (for final-hand settlement).
    pub allow_finished_phase: bool,
    /// Optional target seat this banter line is replying to or nudging.
    pub reply_to_seat: Option<Seat>,
    /// Optional target seat generation guard (invalidates in-flight nudges if target's seat generation bumps,
    /// e.g. toggling `AI 托管` off or offline takeover).
    pub target_seat_generation: Option<(Seat, u64)>,
    /// If `true`, drop the line if `slow_think_nudge_enabled` was toggled off while in-flight.
    pub require_nudge_enabled: bool,
}

/// Scheduled virtual-clock-friendly timer ticket for a single human turn's slow-think nudge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlowThinkTicket {
    pub match_id: u64,
    pub hand_generation: u64,
    pub seq: u64,
    pub target_seat: Seat,
    pub target_seat_generation: u64,
    pub timer_generation: u64,
    pub fire_at: tokio::time::Instant,
}

/// Identifies the specific human turn already nudged so each turn fires at most once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NudgeTurnKey {
    pub match_id: u64,
    pub hand_generation: u64,
    pub event_seq: u64,
    pub seat: Seat,
}

/// Cohesive banter, single-in-flight tracking, quick-emote, slow-think nudge, and LLM outage state
/// owned by `TableSession`.
pub(crate) struct BanterSessionState {
    pub pending_async: Vec<PendingBanterJob>,
    pub pending_memory_notes: Vec<MemoryNoteJob>,
    in_flight_seats: [Option<u64>; 4],
    next_job_id: u64,
    pub llm_available: bool,
    pub last_llm_error: Option<String>,
    pub slow_think_nudge_enabled: bool,
    quick_emote_last_sent: [Option<tokio::time::Instant>; 4],
    active_slow_think_nudge: Option<SlowThinkTicket>,
    last_nudged_turn: Option<NudgeTurnKey>,
    next_nudge_generation: u64,
}

impl BanterSessionState {
    pub(crate) fn new(slow_think_nudge_enabled: bool) -> Self {
        Self {
            pending_async: Vec::new(),
            pending_memory_notes: Vec::new(),
            in_flight_seats: [None; 4],
            next_job_id: 1,
            llm_available: true,
            last_llm_error: None,
            slow_think_nudge_enabled,
            quick_emote_last_sent: [None; 4],
            active_slow_think_nudge: None,
            last_nudged_turn: None,
            next_nudge_generation: 1,
        }
    }

    pub(crate) fn reset_for_match_transition(&mut self) {
        self.pending_async.clear();
        self.in_flight_seats = [None; 4];
        self.quick_emote_last_sent = [None; 4];
        self.active_slow_think_nudge = None;
        self.last_nudged_turn = None;
    }

    pub(crate) fn in_flight_flags(&self) -> [bool; 4] {
        self.in_flight_seats.map(|slot| slot.is_some())
    }

    /// Claims the single in-flight banter slot for `seat`, returning a monotonic `job_id`,
    /// or `None` if `seat` already has an active in-flight task.
    pub(crate) fn try_claim_seat(&mut self, seat: Seat) -> Option<u64> {
        let slot = &mut self.in_flight_seats[seat.index()];
        if slot.is_some() {
            return None;
        }
        let job_id = self.next_job_id;
        self.next_job_id += 1;
        *slot = Some(job_id);
        Some(job_id)
    }

    /// Claims `job.guard.seat` and queues `job` for async execution; silently skips if already busy.
    pub(crate) fn enqueue_banter_job(&mut self, mut job: PendingBanterJob) -> bool {
        let Some(job_id) = self.try_claim_seat(job.guard.seat) else {
            return false;
        };
        job.guard.job_id = job_id;
        self.pending_async.push(job);
        true
    }

    /// Releases `seat`'s in-flight slot only if it still matches `job_id` (stale jobs never clear newer slots).
    pub(crate) fn release_seat(&mut self, seat: Seat, job_id: u64) {
        if job_id != 0 && self.in_flight_seats[seat.index()] == Some(job_id) {
            self.in_flight_seats[seat.index()] = None;
        }
    }

    pub(crate) fn clear_in_flight_seat(&mut self, seat: Seat) {
        self.in_flight_seats[seat.index()] = None;
    }

    pub(crate) fn clear_all_in_flight(&mut self) {
        self.in_flight_seats = [None; 4];
    }

    pub(crate) fn set_slow_think_nudge_enabled(&mut self, enabled: bool) {
        self.slow_think_nudge_enabled = enabled;
        if !enabled {
            self.active_slow_think_nudge = None;
        }
    }

    pub(crate) fn clear_active_nudge(&mut self) {
        self.active_slow_think_nudge = None;
    }

    pub(crate) fn clear_nudge_for_seat(&mut self, seat: Seat) {
        if self
            .active_slow_think_nudge
            .is_some_and(|ticket| ticket.target_seat == seat)
        {
            self.active_slow_think_nudge = None;
        }
    }

    pub(crate) fn try_record_quick_emote(&mut self, seat: Seat, now: tokio::time::Instant) -> bool {
        if self.quick_emote_last_sent[seat.index()]
            .is_some_and(|last| now.saturating_duration_since(last) < QUICK_EMOTE_COOLDOWN)
        {
            return false;
        }
        self.quick_emote_last_sent[seat.index()] = Some(now);
        true
    }

    pub(crate) fn current_outage_notice(&self) -> Option<ServerMessage> {
        if self.llm_available {
            return None;
        }
        self.last_llm_error
            .as_ref()
            .map(|msg| ServerMessage::LlmStatusNotice {
                available: false,
                message: msg.clone(),
            })
    }

    pub(crate) fn record_llm_failure(&mut self, err_display: &str) -> Option<ServerMessage> {
        if !self.llm_available {
            return None;
        }
        let message =
            format!("局域网推理服务异常：{err_display}（牌局照常进行，暂不生成牌桌台词）");
        self.llm_available = false;
        self.last_llm_error = Some(message.clone());
        Some(ServerMessage::LlmStatusNotice {
            available: false,
            message,
        })
    }

    pub(crate) fn record_llm_recovery(&mut self) -> Option<ServerMessage> {
        if self.llm_available {
            return None;
        }
        self.llm_available = true;
        self.last_llm_error = None;
        Some(ServerMessage::LlmStatusNotice {
            available: true,
            message: "局域网推理服务已恢复连接".to_string(),
        })
    }

    pub(crate) fn schedule_slow_think_nudge(
        &mut self,
        turn: NudgeTurnKey,
        target_seat_generation: u64,
    ) -> Option<SlowThinkTicket> {
        if self.last_nudged_turn == Some(turn) {
            self.active_slow_think_nudge = None;
            return None;
        }
        if self.active_slow_think_nudge.is_some_and(|existing| {
            existing.match_id == turn.match_id
                && existing.hand_generation == turn.hand_generation
                && existing.seq == turn.event_seq
                && existing.target_seat == turn.seat
                && existing.target_seat_generation == target_seat_generation
        }) {
            return None;
        }

        let timer_generation = self.next_nudge_generation;
        self.next_nudge_generation += 1;
        let ticket = SlowThinkTicket {
            match_id: turn.match_id,
            hand_generation: turn.hand_generation,
            seq: turn.event_seq,
            target_seat: turn.seat,
            target_seat_generation,
            timer_generation,
            fire_at: tokio::time::Instant::now() + SLOW_THINK_NUDGE_DELAY,
        };
        self.active_slow_think_nudge = Some(ticket);
        Some(ticket)
    }

    pub(crate) fn consume_matching_nudge(&mut self, ticket: SlowThinkTicket) -> bool {
        if self.active_slow_think_nudge != Some(ticket) {
            return false;
        }
        self.active_slow_think_nudge = None;
        true
    }

    pub(crate) fn is_turn_already_nudged(&self, turn: NudgeTurnKey) -> bool {
        self.last_nudged_turn == Some(turn)
    }

    pub(crate) fn mark_turn_nudged(&mut self, turn: NudgeTurnKey) {
        self.last_nudged_turn = Some(turn);
    }

    pub(crate) fn take_pending(&mut self) -> (Vec<PendingBanterJob>, Vec<MemoryNoteJob>) {
        (
            std::mem::take(&mut self.pending_async),
            std::mem::take(&mut self.pending_memory_notes),
        )
    }
}

/// Structured, speaker-neutral interaction trigger stored in a serial follow-up chain so every
/// follow-up AI formats relative seat labels (`自家`/`上家`/`下家`/`对门`) strictly from its own seat perspective.
#[derive(Debug, Clone)]
pub(crate) enum BanterChainTrigger {
    HandSettled(Box<HandSettlement>),
    MeldDeclared {
        actor: Seat,
        /// Privacy-sanitized meld view (`ConcealedKong.tile` is always `None`).
        meld: MeldView,
        wall_remaining: u8,
    },
    AiDiscard {
        actor: Seat,
        tile: Tile,
        wall_remaining: u8,
    },
    QuickEmote {
        sender: Seat,
        emote: QuickEmote,
    },
    SlowThinkNudge {
        target_seat: Seat,
    },
}

/// One committed utterance within an active serial interaction chain.
#[derive(Debug, Clone)]
pub(crate) struct ChainUtterance {
    pub seat: Seat,
    pub mood: PersonaMood,
    pub text: String,
}

/// Active serial AI-to-AI follow-up chain context attached to an autonomous banter job.
#[derive(Debug, Clone)]
pub(crate) struct BanterChainContext {
    /// Structured trigger formatted relative to each follow-up speaker (never leaks hidden tiles).
    pub trigger: BanterChainTrigger,
    /// True only when all four hands are publicly revealed (`HandSettled`).
    pub hands_revealed: bool,
    /// Number of follow-up replies already emitted in this chain (`0` for the opening line).
    pub replies_emitted: u8,
    /// Maximum follow-up replies permitted for this chain (`1..=MAX_BANTER_CHAIN_REPLIES`).
    pub max_replies: u8,
    /// Seats that have already spoken in this chain.
    pub spoken_seats: Vec<Seat>,
    /// Chronological utterances committed earlier in this chain (carried into follow-up prompts).
    pub history: Vec<ChainUtterance>,
}

/// Previous line emitted in an AI banter chain, passed to `plan_chain_followup`.
pub(crate) struct ChainPrevLine<'a> {
    pub guard: BanterGuard,
    pub mood: PersonaMood,
    pub text: &'a str,
}

/// Scheduled non-blocking LLM banter job ready for asynchronous execution.
#[derive(Debug, Clone)]
pub(crate) struct PendingBanterJob {
    pub guard: BanterGuard,
    pub request: BanterGenerationRequest,
    pub dispatch_delay: Option<Duration>,
    pub chain: Option<BanterChainContext>,
}

/// Snapshot of table identity needed to plan autonomous banter without holding `TableSession` mutably.
pub(crate) struct BanterPlanContext<'a> {
    pub match_id: u64,
    pub hand_generation: u64,
    pub game: &'a ActiveMatch,
    pub seats: &'a [SeatInfo; 4],
    pub seat_generation: &'a [u64; 4],
    pub in_flight_seats: [bool; 4],
    pub persona_memory: &'a PersonaMemoryStore,
    pub banter_context: &'a MatchBanterContext,
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

pub(crate) fn format_mood_zh(mood: PersonaMood) -> &'static str {
    match mood {
        PersonaMood::Smug => "得意欠揍",
        PersonaMood::Mocking => "阴阳嘲讽",
        PersonaMood::Tilted => "红温破防",
        PersonaMood::Nervous => "心虚紧绷",
        PersonaMood::Plotting => "阴险憋大招",
        PersonaMood::Deadpan => "无语装淡定",
    }
}

/// Formats precomputed [`HandFacts`] into a concise, authoritative Chinese summary for prompt injection.
#[must_use]
pub fn format_hand_facts_zh(facts: &HandFacts) -> String {
    let mut out = String::new();
    if facts.is_winning {
        out.push_str("已成胡牌形（可直接自摸）");
    } else if facts.is_tenpai {
        if let Some(discard) = facts.best_discard {
            let _ = write!(out, "0向听（舍出{}即可听牌）", format_tile_zh(discard));
        } else {
            out.push_str("已听牌（0向听）");
        }
        if !facts.waits.is_empty() {
            let wait_parts: Vec<String> = facts
                .waits
                .iter()
                .map(|w| {
                    format!(
                        "{}(余{}张/{}番)",
                        format_tile_zh(w.tile),
                        w.remaining_visible,
                        w.winning_fan
                    )
                })
                .collect();
            let _ = write!(out, "，听口：{}", wait_parts.join("、"));
            if facts.ukeire == 0 {
                out.push_str("（公开可见余0张，已绝张空听）");
            } else {
                let _ = write!(out, "（公开可见共{}张）", facts.ukeire);
            }
        }
    } else {
        let _ = write!(out, "{}向听，公开有效进张{}张", facts.shanten, facts.ukeire);
    }

    if !facts.potential_directions.is_empty() {
        let dirs: Vec<&str> = facts
            .potential_directions
            .iter()
            .map(|d| d.label_zh())
            .collect();
        let _ = write!(out, "；大番方向：{}", dirs.join("、"));
    }
    if facts.is_menqing {
        out.push_str("；门前清状态");
    }

    match facts.progression {
        HandProgression::Initial => {}
        HandProgression::CanWin => out.push_str("；本手进展：刚摸到胡牌张"),
        HandProgression::EnteredTenpai => out.push_str("；本手进展：刚刚听牌"),
        HandProgression::Improved => out.push_str("；本手进展：手牌进张提速"),
        HandProgression::Stalled => {
            if facts.stalled_streak >= 2 {
                let _ = write!(
                    out,
                    "；本手进展：已连续{}手无效进张卡手",
                    facts.stalled_streak
                );
            } else {
                out.push_str("；本手进展：本手原地踏步");
            }
        }
        HandProgression::Regressed => {
            if facts.stalled_streak >= 2 {
                let _ = write!(
                    out,
                    "；本手进展：进张变窄且已连续{}手未推进",
                    facts.stalled_streak
                );
            } else {
                out.push_str("；本手进展：手牌退步或进张变窄");
            }
        }
    }

    out
}

pub(crate) fn format_own_hand(view: &PlayerView) -> String {
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
        seat_direction_and_name(seats, speaker, revealed.seat),
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

/// Returns the rich, humanized everyday-acquaintance profile and speech habits for a built-in persona.
pub(crate) fn persona_voice_guide(persona: PersonaId) -> &'static str {
    match persona {
        PersonaId::QuickHothead => {
            "你是熟人牌友「大川」：川渝急性子爽快哥们，打牌爱快攻抢节奏、手速嘴速双快。性格直来直去又讲义气，牌顺了忍不住洋洋得意吹两句，连卡几巡或被抢碰立刻急眼炸毛，但气来得快去得也快。说话干脆利落带火药味，偶尔自然带点川渝口语味（像“要得”“莫急嘛”“稳得很”“搞快点”），绝不尬演方言腔；平常爱聊火锅串串、夜宵宵夜和手气玄学。"
        }
        PersonaId::TeaSarcasm => {
            "你是熟人牌友「小晚」：温婉腹黑、笑里藏刀的姐妹，外表客气斯文、说话慢条斯理轻声细语，实则嘴毒扎心不带脏字。牌风稳健爱憋大牌，记忆力极好又爱记仇，最擅长笑眯眯翻旧账戳肺管子。口头习惯用“没事呀”“挺好的”“你开心就好”“我也没说啥”这类轻飘飘的反话拿捏人；日常爱聊奶茶甜品、追剧吃瓜和上班摸鱼。"
        }
        PersonaId::StubbornBlamer => {
            "你是熟人牌友「大鹏」：京片子嘴贫死磕的铁哥们，爱打快攻、死要面子绝不认怂。赢了就吹成算无遗策，点炮或牌碎永远嘴硬不服输，总能掰出各种清奇借口（怪风水、怪节奏、怪上家乱出、怪今天没吃饱），输赢其实看得很开但嘴上绝不让步。说话带点爽朗京味贫嘴和冷幽默（像“这叫战术”“没毛病”“就差一丢丢”）；平常爱聊烧烤夜宵、加班摸鱼和转运玄学。"
        }
        PersonaId::ChaosInstigator => {
            "你是熟人牌友「小桃」：网感极强、爱吃瓜起哄补刀的损友妹妹，牌风均衡机灵。看别人点炮和互怼比自家胡牌还来劲，最擅长在牌桌气氛微妙时递刀子、拱火、顺茬接梗和神补刀，脑洞跳脱反应极快。说话活泼轻快带自然年轻口语（绝不尬套过时老梗）；日常爱聊外卖奶茶、熬夜刷手机、周末躺平和生活小吐槽。"
        }
        PersonaId::TiltedMaster => {
            "你是熟人牌友「老陈」：爱端前辈架子的熟人老哥，自诩老江湖、防守做大牌派，总忍不住好为人师点评他家打法、传授“老经验”。顺风时一脸高深莫测，可一旦自家卡手、被截胡或点炮立刻端不住架子，当场血压飙升秒红温破防，随后还要强装镇定找补挽尊。口语常带“按我说”“听哥一句”“这牌打得…”的过来人腔调；日常爱聊茶酒下酒菜、早起养生和工作琐事。"
        }
    }
}

/// Builds the humanized system prompt for autonomous `{ speak, mood, text }` banter decisions (`ADR-0015`).
#[must_use]
pub fn build_system_prompt(persona: PersonaId) -> String {
    format!(
        "{voice}\n\
你在家庭熟人推倒胡麻将牌桌上边打牌边唠嗑，风格是真实熟人损友之间的自然口语互动。\n\
必须严格遵守：\n\
1. 仅输出合法 JSON：{{\"speak\":true/false,\"mood\":\"...\",\"text\":\"...\"}}，严禁在最终正文中输出 <think> 标签、代码块或多余解释（speak 为 false 时 text 必须为空字符串 \"\"）。\n\
2. 自主拿捏开口分寸：普通无波澜的摸牌出牌回合，优先保持沉默（speak: false）；心里情绪变了可以沉默只切 mood 变脸。真有牌运起伏、绝妙接梗、想拆台补刀、或想顺势闲聊时再开口（speak: true）；结合「发言间隔提示」克制频率，刚开过口没新梗别硬接。\n\
3. mood 必须从以下 6 档心理锚点中选最贴合当下处境的一项：\n\
- Smug：得意欠揍（胡牌、占便宜、牌顺、稳操胜券）\n\
- Mocking：阴阳嘲讽（笑话别人点炮、调侃他家操作、煽风点火）\n\
- Tilted：红温破防（被抢碰、被抢杠胡、点炮赔分、连续卡手气急败坏）\n\
- Nervous：心虚紧绷（场面危险、被逼紧、怕点炮、听口悬乎）\n\
- Plotting：阴险憋大招（暗自算计、坐山观虎斗、憋大番准备阴人）\n\
- Deadpan：无语装淡定（荒庄无语、强行嘴硬挽尊、冷眼旁观）\n\
4. text 为 2 到 18 字的自然中文口语短句（常态 4 到 14 字脱口而出，允许极短感叹如“绝了”“稳了”，也允许偶尔完整抖包袱），直接对牌桌说话，不换行、不书面腔、不客服腔、不报菜名。\n\
5. 称呼自然不点名：默认直接开口，不要在句首硬加对方姓名！像真人打牌一样直接感叹、自言自语，或自然用“你”“上家”“老哥”“姐们”等称呼；只在真需要点名调侃、算旧账或点炮追责时，才顺口提到对方昵称，切忌照抄“方位+姓名”标签。\n\
6. 熟人日常闲聊：约 15% 到 25% 的开口可以结合「现实时间语境」（饭点、夜宵、加班摸鱼、熬夜、周末安排）或手气玄学聊点轻松的牌外闲话与接地气玩梗，也欢迎顺着「牌桌对话流」里别人刚起的话头接茬互逗，别复读刚说过的梗；严禁低俗脏话、敏感时政与客套客服腔。\n\
7. 事实与隐私红线：仅可引用输入事实中明确出现的牌名、玩家昵称与番型，未给出具体牌名时只能泛指，严禁捏造未出现的牌名或臆测他家暗牌（含他家暗杠）；必须严格遵循「预计算手牌事实」中的向听、听口与番型方向，绝不胡算牌，结合自身性格找丰富真实的牌桌吐槽或找补角度。",
        voice = persona_voice_guide(persona),
    )
}

fn idle_ai_persona_seats(
    seats: &[SeatInfo; 4],
    in_flight_seats: &[bool; 4],
) -> Vec<(Seat, PersonaId)> {
    seats
        .iter()
        .filter_map(|info| {
            (info.kind == SeatKind::Ai && !in_flight_seats[info.seat.index()])
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

fn find_idle_ai_persona(
    seats: &[SeatInfo; 4],
    in_flight_seats: &[bool; 4],
    seat: Seat,
) -> Option<(Seat, PersonaId)> {
    if in_flight_seats[seat.index()] {
        return None;
    }
    find_ai_persona(seats, seat)
}

fn format_public_meld_summary(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    actor: Seat,
    meld: &MeldView,
    reveal_own_concealed: bool,
) -> String {
    let actor_label = seat_direction_and_name(seats, speaker, actor);
    match meld {
        MeldView::Pon {
            tile, from_seat, ..
        } => format!(
            "{actor_label} 碰了 {} 的 {}",
            seat_direction_and_name(seats, speaker, *from_seat),
            format_tile_zh(*tile)
        ),
        MeldView::ExposedKong {
            tile, from_seat, ..
        } => format!(
            "{actor_label} 明杠了 {} 的 {}",
            seat_direction_and_name(seats, speaker, *from_seat),
            format_tile_zh(*tile)
        ),
        MeldView::AddedKong { tile, .. } => {
            format!("{actor_label} 补杠了 {}", format_tile_zh(*tile))
        }
        MeldView::ConcealedKong { tile, .. } => match tile {
            Some(t) if reveal_own_concealed && speaker == actor => {
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
            seat_direction_and_name(seats, speaker, *from_seat),
            format_tile_zh(*called_tile),
            format_tiles_zh(tiles)
        ),
    }
}

fn compose_scene_and_six_blocks(
    scene: String,
    banter_context: &MatchBanterContext,
    input: &SixBlockInput<'_>,
) -> String {
    let six_blocks = banter_context.format_six_blocks(input);
    format!("{scene}\n\n{six_blocks}")
}

fn build_ai_discard_checkin_prompt(discarded_tile: Tile) -> String {
    format!(
        "【自家出牌巡检】你刚刚舍出 {}。\n\
请结合下方 6 块牌桌上下文（整场战况、公开牌河、自家手牌与牌运轨迹、对话流、牌友记忆与当前时间），自主决定此刻是否开口（speak: true/false）以及牌友情绪（mood）：\
若本巡平淡无波澜优先保持沉默（speak: false，情绪有变化时仅静默切 mood）；\
若有牌运起伏、听牌/做大牌心理战、想顺着对话流接梗或聊一句轻松的日常闲话，则脱口来一句自然短句。",
        format_tile_zh(discarded_tile),
    )
}

fn build_meld_declared_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    actor: Seat,
    meld: &MeldView,
    wall_remaining: u8,
) -> String {
    let action_desc = format_public_meld_summary(seats, speaker, actor, meld, true);
    format!(
        "【鸣牌动态】{action_desc}，牌墙剩余 {wall_remaining} 张。\n\
请结合下方 6 块牌桌上下文自主决定是否就这次鸣牌开口（speak: true/false）以及当前牌友情绪（mood）：想接话就自然吐槽、调侃或施压，没必要说话就保持沉默。"
    )
}

fn format_chain_trigger_summary(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    trigger: &BanterChainTrigger,
) -> String {
    match trigger {
        BanterChainTrigger::HandSettled(settlement) => {
            let (outcome_summary, deltas, revealed) =
                format_settlement_summary_lines(seats, speaker, settlement);
            format!("{outcome_summary} 得分：{deltas}。\n四家摊开手牌：\n{revealed}")
        }
        BanterChainTrigger::MeldDeclared {
            actor,
            meld,
            wall_remaining,
        } => format!(
            "{}（牌墙余{wall_remaining}张）",
            format_public_meld_summary(seats, speaker, *actor, meld, false),
        ),
        BanterChainTrigger::AiDiscard {
            actor,
            tile,
            wall_remaining,
        } => format!(
            "{} 舍出 {}（牌墙余{wall_remaining}张）",
            seat_direction_and_name(seats, speaker, *actor),
            format_tile_zh(*tile),
        ),
        BanterChainTrigger::QuickEmote { sender, emote } => format!(
            "{} 发送快捷互动：「{}」",
            seat_direction_and_name(seats, speaker, *sender),
            emote.label(),
        ),
        BanterChainTrigger::SlowThinkNudge { target_seat } => format!(
            "{} 这一手思考了较久",
            seat_direction_and_name(seats, speaker, *target_seat),
        ),
    }
}

fn build_chain_reply_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    history: &[ChainUtterance],
    chain: &BanterChainContext,
) -> String {
    let summary = format_chain_trigger_summary(seats, speaker, &chain.trigger);
    let mut prompt = format!("【牌友接茬判断】当前场面：{summary}\n刚刚同段发言：\n");
    for utt in history {
        let label = seat_direction_and_name(seats, speaker, utt.seat);
        let _ = writeln!(
            prompt,
            "- {label}（{}）：「{text}」",
            format_mood_zh(utt.mood),
            text = utt.text,
        );
    }
    prompt.push_str(
        "请结合下方 6 块牌桌上下文自主决定要不要顺着刚才的话接茬回喷、拆台、补刀或接住日常闲聊（speak: true/false）；没梗可接就安静返回 speak: false，若开口严禁捏造未给出的牌名。",
    );
    prompt
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
                "{} {kind}（{delta:+}分）",
                seat_direction_and_name(seats, speaker, k.actor),
            )
        })
        .collect::<Vec<_>>()
        .join("、")
}

fn format_settlement_summary_lines(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    settlement: &HandSettlement,
) -> (String, String, String) {
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
                    "{} {win_kind} {tile_name}（共{}番：{items}）",
                    seat_direction_and_name(seats, speaker, w.seat),
                    w.total_fan,
                ));
            }
            let multi = if winners.len() > 1 {
                "一炮多响！"
            } else {
                ""
            };
            let discarder_note = match discarder {
                Some(d) => format!(
                    "，点炮/被抢杠胡者：{}",
                    seat_direction_and_name(seats, speaker, *d)
                ),
                None => String::new(),
            };
            format!("{multi}{}{discarder_note}。", parts.join("；"))
        }
    };

    let deltas = Seat::ALL
        .into_iter()
        .map(|s| {
            format!(
                "{} {:+}分（现{}分）",
                seat_direction_and_name(seats, speaker, s),
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

    (outcome_summary, deltas, revealed)
}

fn build_hand_settled_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    settlement: &HandSettlement,
) -> String {
    let (outcome_summary, deltas, revealed) =
        format_settlement_summary_lines(seats, speaker, settlement);
    let speaker_delta = settlement.score_deltas[speaker.index()];
    format!(
        "【小局结算复盘】{outcome_summary}\n\
杠分明细：{kongs}。\n\
本小局得分变化：{deltas}（你本小局净变化：{speaker_delta:+}分）。\n\
四家摊开手牌（已全部公开，可据此精准复盘互喷）：\n\
{revealed}\n\
请结合下方 6 块整场上下文自主决定是否点评本小局胜负、番型或真实摊牌内容（speak: true/false）以及当前牌友情绪（mood）。",
        kongs = format_kong_transfers(seats, speaker, settlement),
    )
}

fn build_quick_emote_reply_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    sender: Seat,
    emote: QuickEmote,
) -> String {
    let sender_label = seat_direction_and_name(seats, speaker, sender);
    let emote_text = emote.label();
    format!(
        "【快捷互动回应】{sender_label} 刚向全桌发了快捷互动：「{emote_text}」。\n\
请结合下方 6 块牌桌上下文自主决定是否顺着这句「{emote_text}」接话回喷或调侃（speak: true/false）以及当前牌友情绪（mood）。"
    )
}

fn build_slow_think_nudge_prompt(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    target_seat: Seat,
) -> String {
    let target_label = seat_direction_and_name(seats, speaker, target_seat);
    format!(
        "【慢思考催牌调侃】{target_label} 这一手已经想了快二十秒还没操作。\n\
请结合下方 6 块牌桌上下文自主决定是否来一句俏皮温和的催牌调侃（speak: true/false，带点损友玩笑催一催，别太凶也别催命）。"
    )
}

/// Plans 1 asynchronous autonomous banter check-in for an idle AI reacting to a human `QuickEmote`.
pub(crate) fn plan_quick_emote_reply(
    ctx: &BanterPlanContext<'_>,
    sender: Seat,
    emote: QuickEmote,
) -> Option<PendingBanterJob> {
    let idle_ai = idle_ai_persona_seats(ctx.seats, &ctx.in_flight_seats);
    let seq = ctx.game.seq();
    let seed = ctx
        .match_id
        .wrapping_mul(17)
        .wrapping_add(seq)
        .wrapping_add(emote.index() as u64);
    let (speaker, persona_id) = pick_deterministic_ai(&idle_ai, seed)?;
    let view = ctx.game.project_view(speaker);
    let facts = analyze_hand_facts(&view, ctx.banter_context.last_hand_facts(speaker));
    let local_time = current_local_time_zh();
    let user_prompt = compose_scene_and_six_blocks(
        build_quick_emote_reply_prompt(ctx.seats, speaker, sender, emote),
        ctx.banter_context,
        &SixBlockInput {
            speaker,
            persona_id,
            seats: ctx.seats,
            view: &view,
            facts: Some(&facts),
            hands_revealed: false,
            persona_memory: ctx.persona_memory,
            local_time_zh: &local_time,
        },
    );
    let request = BanterGenerationRequest::new(build_system_prompt(persona_id), user_prompt);
    Some(PendingBanterJob {
        guard: BanterGuard {
            job_id: 0,
            match_id: ctx.match_id,
            seat: speaker,
            seat_generation: ctx.seat_generation[speaker.index()],
            persona_id,
            event_seq: seq,
            hand_generation: ctx.hand_generation,
            max_seq_drift: Some(STRONG_RECENCY_MAX_SEQ_DRIFT),
            allow_finished_phase: false,
            reply_to_seat: Some(sender),
            target_seat_generation: None,
            require_nudge_enabled: false,
        },
        request,
        dispatch_delay: None,
        chain: Some(BanterChainContext {
            trigger: BanterChainTrigger::QuickEmote { sender, emote },
            hands_revealed: false,
            replies_emitted: 0,
            max_replies: MAX_BANTER_CHAIN_REPLIES,
            spoken_seats: vec![speaker],
            history: Vec::new(),
        }),
    })
}

/// Plans 1 playful asynchronous autonomous nudge check-in for an idle AI when a human thinks past threshold.
pub(crate) fn plan_slow_think_nudge(
    ctx: &BanterPlanContext<'_>,
    target_seat: Seat,
    random_pick: u64,
) -> Option<PendingBanterJob> {
    let idle_ai = idle_ai_persona_seats(ctx.seats, &ctx.in_flight_seats);
    let (speaker, persona_id) = pick_deterministic_ai(&idle_ai, random_pick)?;
    let seq = ctx.game.seq();
    let view = ctx.game.project_view(speaker);
    let facts = analyze_hand_facts(&view, ctx.banter_context.last_hand_facts(speaker));
    let local_time = current_local_time_zh();
    let user_prompt = compose_scene_and_six_blocks(
        build_slow_think_nudge_prompt(ctx.seats, speaker, target_seat),
        ctx.banter_context,
        &SixBlockInput {
            speaker,
            persona_id,
            seats: ctx.seats,
            view: &view,
            facts: Some(&facts),
            hands_revealed: false,
            persona_memory: ctx.persona_memory,
            local_time_zh: &local_time,
        },
    );
    let request = BanterGenerationRequest::new(build_system_prompt(persona_id), user_prompt);
    Some(PendingBanterJob {
        guard: BanterGuard {
            job_id: 0,
            match_id: ctx.match_id,
            seat: speaker,
            seat_generation: ctx.seat_generation[speaker.index()],
            persona_id,
            event_seq: seq,
            hand_generation: ctx.hand_generation,
            max_seq_drift: Some(STRONG_RECENCY_MAX_SEQ_DRIFT),
            allow_finished_phase: false,
            reply_to_seat: Some(target_seat),
            target_seat_generation: Some((target_seat, ctx.seat_generation[target_seat.index()])),
            require_nudge_enabled: true,
        },
        request,
        dispatch_delay: None,
        chain: Some(BanterChainContext {
            trigger: BanterChainTrigger::SlowThinkNudge { target_seat },
            hands_revealed: false,
            replies_emitted: 0,
            max_replies: MAX_BANTER_CHAIN_REPLIES,
            spoken_seats: vec![speaker],
            history: Vec::new(),
        }),
    })
}

/// Plans at most 1 non-blocking autonomous banter check-in for an emitted event batch:
/// - `HandStarted`: no forced opening banter (trajectories already reset by `MatchBanterContext::observe_events`).
/// - `HandSettled`: wakes 1 most relevant idle AI (`max_seq_drift: None`, `allow_finished_phase: true`).
/// - Human `MeldDeclared` (`Pon`/`ExposedKong`/`AddedKong`/`ConcealedKong`): wakes 1 most relevant idle AI
///   (`max_seq_drift: Some(STRONG_RECENCY_MAX_SEQ_DRIFT)`).
/// - AI `TileDiscarded`: wakes that AI if idle (`max_seq_drift: None`).
/// - `TileDrawn`, human `TileDiscarded`, AI `MeldDeclared`, etc.: never wake banter.
pub(crate) fn plan_banter_for_events(
    ctx: &BanterPlanContext<'_>,
    emitted: &[RecordedEvent],
) -> Option<PendingBanterJob> {
    if emitted.is_empty() {
        return None;
    }

    // 1. HandStarted: never force opening banter.
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if matches!(projected.kind, PlayerEventKind::HandStarted { .. }) {
            return None;
        }
    }

    let idle_ai = idle_ai_persona_seats(ctx.seats, &ctx.in_flight_seats);
    let local_time = current_local_time_zh();

    // 2. HandSettled (Tsumo / Ron / RobKongHu / DrawGame): non-blocking async wake-up of 1 idle AI.
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if let PlayerEventKind::HandSettled { settlement } = projected.kind {
            let (seat, persona_id) = pick_settlement_speaker(
                ctx.seats,
                &ctx.in_flight_seats,
                &idle_ai,
                &settlement,
                event.seq(),
            )?;
            let view = ctx.game.project_view(seat);
            let user_prompt = compose_scene_and_six_blocks(
                build_hand_settled_prompt(ctx.seats, seat, &settlement),
                ctx.banter_context,
                &SixBlockInput {
                    speaker: seat,
                    persona_id,
                    seats: ctx.seats,
                    view: &view,
                    facts: None,
                    hands_revealed: true,
                    persona_memory: ctx.persona_memory,
                    local_time_zh: &local_time,
                },
            );
            let request =
                BanterGenerationRequest::new(build_system_prompt(persona_id), user_prompt);
            return Some(PendingBanterJob {
                guard: BanterGuard {
                    job_id: 0,
                    match_id: ctx.match_id,
                    seat,
                    seat_generation: ctx.seat_generation[seat.index()],
                    persona_id,
                    event_seq: event.seq(),
                    hand_generation: ctx.hand_generation,
                    max_seq_drift: None,
                    allow_finished_phase: true,
                    reply_to_seat: None,
                    target_seat_generation: None,
                    require_nudge_enabled: false,
                },
                request,
                dispatch_delay: None,
                chain: Some(BanterChainContext {
                    trigger: BanterChainTrigger::HandSettled(Box::new(settlement)),
                    hands_revealed: true,
                    replies_emitted: 0,
                    max_replies: MAX_BANTER_CHAIN_REPLIES,
                    spoken_seats: vec![seat],
                    history: Vec::new(),
                }),
            });
        }
    }

    // 3. Human MeldDeclared: wake 1 relevant idle AI ONLY for human melds/kongs.
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if let PlayerEventKind::MeldDeclared {
            seat: actor, meld, ..
        } = projected.kind
        {
            if ctx.seats[actor.index()].kind != SeatKind::Human {
                continue;
            }
            let (speaker, persona_id) = pick_human_meld_speaker(
                ctx.seats,
                &ctx.in_flight_seats,
                &idle_ai,
                &meld,
                event.seq(),
            )?;
            // Project strictly for the speaking seat so opponent ConcealedKong tiles stay hidden.
            let speaker_event = ctx.game.project_event(event, speaker);
            let speaker_meld = match speaker_event.kind {
                PlayerEventKind::MeldDeclared { meld, .. } => meld,
                _ => meld,
            };
            let view = ctx.game.project_view(speaker);
            let facts = analyze_hand_facts(&view, ctx.banter_context.last_hand_facts(speaker));
            let user_prompt = compose_scene_and_six_blocks(
                build_meld_declared_prompt(
                    ctx.seats,
                    speaker,
                    actor,
                    &speaker_meld,
                    view.wall_remaining,
                ),
                ctx.banter_context,
                &SixBlockInput {
                    speaker,
                    persona_id,
                    seats: ctx.seats,
                    view: &view,
                    facts: Some(&facts),
                    hands_revealed: false,
                    persona_memory: ctx.persona_memory,
                    local_time_zh: &local_time,
                },
            );
            let request =
                BanterGenerationRequest::new(build_system_prompt(persona_id), user_prompt);
            // Privacy-sanitized meld for follow-up AI speakers (never exposes ConcealedKong tile!).
            let public_meld = match speaker_meld {
                MeldView::ConcealedKong { seq, .. } => MeldView::ConcealedKong { tile: None, seq },
                other => other,
            };
            return Some(PendingBanterJob {
                guard: BanterGuard {
                    job_id: 0,
                    match_id: ctx.match_id,
                    seat: speaker,
                    seat_generation: ctx.seat_generation[speaker.index()],
                    persona_id,
                    event_seq: event.seq(),
                    hand_generation: ctx.hand_generation,
                    max_seq_drift: Some(STRONG_RECENCY_MAX_SEQ_DRIFT),
                    allow_finished_phase: false,
                    reply_to_seat: None,
                    target_seat_generation: None,
                    require_nudge_enabled: false,
                },
                request,
                dispatch_delay: None,
                chain: Some(BanterChainContext {
                    trigger: BanterChainTrigger::MeldDeclared {
                        actor,
                        meld: public_meld,
                        wall_remaining: view.wall_remaining,
                    },
                    hands_revealed: false,
                    replies_emitted: 0,
                    max_replies: MAX_BANTER_CHAIN_REPLIES,
                    spoken_seats: vec![speaker],
                    history: Vec::new(),
                }),
            });
        }
    }

    // 4. Routine self-check-in: 1 async check-in after an AI completes its own discard (never on draw).
    for event in emitted {
        let projected = ctx.game.project_event(event, Seat::East);
        if let PlayerEventKind::TileDiscarded { seat, tile, .. } = projected.kind {
            if let Some((speaker, persona_id)) = find_ai_persona(ctx.seats, seat) {
                if ctx.in_flight_seats[speaker.index()] {
                    return None;
                }
                let view = ctx.game.project_view(speaker);
                let facts = ctx
                    .banter_context
                    .last_hand_facts(speaker)
                    .cloned()
                    .unwrap_or_else(|| analyze_hand_facts(&view, None));
                let user_prompt = compose_scene_and_six_blocks(
                    build_ai_discard_checkin_prompt(tile),
                    ctx.banter_context,
                    &SixBlockInput {
                        speaker,
                        persona_id,
                        seats: ctx.seats,
                        view: &view,
                        facts: Some(&facts),
                        hands_revealed: false,
                        persona_memory: ctx.persona_memory,
                        local_time_zh: &local_time,
                    },
                );
                let request =
                    BanterGenerationRequest::new(build_system_prompt(persona_id), user_prompt);
                return Some(PendingBanterJob {
                    guard: BanterGuard {
                        job_id: 0,
                        match_id: ctx.match_id,
                        seat: speaker,
                        seat_generation: ctx.seat_generation[speaker.index()],
                        persona_id,
                        event_seq: event.seq(),
                        hand_generation: ctx.hand_generation,
                        max_seq_drift: None,
                        allow_finished_phase: false,
                        reply_to_seat: None,
                        target_seat_generation: None,
                        require_nudge_enabled: false,
                    },
                    request,
                    dispatch_delay: None,
                    chain: Some(BanterChainContext {
                        trigger: BanterChainTrigger::AiDiscard {
                            actor: speaker,
                            tile,
                            wall_remaining: view.wall_remaining,
                        },
                        hands_revealed: false,
                        replies_emitted: 0,
                        max_replies: MAX_BANTER_CHAIN_REPLIES,
                        spoken_seats: vec![speaker],
                        history: Vec::new(),
                    }),
                });
            }
        }
    }

    None
}

/// Plans the next serial autonomous AI-to-AI follow-up check-in after a committed `TableBanter`,
/// strictly capped at `MAX_BANTER_CHAIN_REPLIES` (2) follow-up rounds, selecting only from idle AI seats
/// that have not yet spoken in this interaction chain.
pub(crate) fn plan_chain_followup(
    ctx: &BanterPlanContext<'_>,
    prev: &ChainPrevLine<'_>,
    chain: BanterChainContext,
    reply_delay: Duration,
) -> Option<PendingBanterJob> {
    let hard_cap = chain.max_replies.min(MAX_BANTER_CHAIN_REPLIES);
    if chain.replies_emitted >= hard_cap {
        return None;
    }

    let eligible: Vec<(Seat, PersonaId)> = idle_ai_persona_seats(ctx.seats, &ctx.in_flight_seats)
        .into_iter()
        .filter(|&(seat, _)| seat != prev.guard.seat && !chain.spoken_seats.contains(&seat))
        .collect();

    let current_seq = ctx.game.seq();
    let (next_seat, next_persona) = pick_deterministic_ai(
        &eligible,
        ctx.match_id
            .wrapping_mul(37)
            .wrapping_add(current_seq)
            .wrapping_add(u64::from(chain.replies_emitted)),
    )?;

    let view = ctx.game.project_view(next_seat);
    let own_facts = if chain.hands_revealed {
        None
    } else {
        Some(analyze_hand_facts(
            &view,
            ctx.banter_context.last_hand_facts(next_seat),
        ))
    };

    let mut next_history = chain.history.clone();
    next_history.push(ChainUtterance {
        seat: prev.guard.seat,
        mood: prev.mood,
        text: prev.text.to_string(),
    });

    let local_time = current_local_time_zh();
    let prompt = compose_scene_and_six_blocks(
        build_chain_reply_prompt(ctx.seats, next_seat, &next_history, &chain),
        ctx.banter_context,
        &SixBlockInput {
            speaker: next_seat,
            persona_id: next_persona,
            seats: ctx.seats,
            view: &view,
            facts: own_facts.as_ref(),
            hands_revealed: chain.hands_revealed,
            persona_memory: ctx.persona_memory,
            local_time_zh: &local_time,
        },
    );

    let mut next_spoken = chain.spoken_seats;
    next_spoken.push(next_seat);

    Some(PendingBanterJob {
        guard: BanterGuard {
            job_id: 0,
            match_id: prev.guard.match_id,
            seat: next_seat,
            seat_generation: ctx.seat_generation[next_seat.index()],
            persona_id: next_persona,
            event_seq: current_seq,
            hand_generation: prev.guard.hand_generation,
            max_seq_drift: if chain.hands_revealed {
                None
            } else {
                Some(STRONG_RECENCY_MAX_SEQ_DRIFT)
            },
            allow_finished_phase: chain.hands_revealed,
            reply_to_seat: Some(prev.guard.seat),
            target_seat_generation: prev.guard.target_seat_generation,
            require_nudge_enabled: prev.guard.require_nudge_enabled,
        },
        request: BanterGenerationRequest::new(build_system_prompt(next_persona), prompt),
        dispatch_delay: Some(reply_delay),
        chain: Some(BanterChainContext {
            trigger: chain.trigger,
            hands_revealed: chain.hands_revealed,
            replies_emitted: chain.replies_emitted + 1,
            max_replies: hard_cap,
            spoken_seats: next_spoken,
            history: next_history,
        }),
    })
}

fn pick_human_meld_speaker(
    seats: &[SeatInfo; 4],
    in_flight_seats: &[bool; 4],
    idle_ai: &[(Seat, PersonaId)],
    meld: &MeldView,
    event_seq: u64,
) -> Option<(Seat, PersonaId)> {
    // Prefer the idle AI victim whose discard was claimed by the human's Pon/ExposedKong/Chi.
    let from_seat = match meld {
        MeldView::Pon { from_seat, .. }
        | MeldView::ExposedKong { from_seat, .. }
        | MeldView::Chi { from_seat, .. } => Some(*from_seat),
        MeldView::AddedKong { .. } | MeldView::ConcealedKong { .. } => None,
    };
    if let Some(victim) = from_seat {
        if let Some(found) = find_idle_ai_persona(seats, in_flight_seats, victim) {
            return Some(found);
        }
    }
    pick_deterministic_ai(idle_ai, event_seq)
}

fn pick_settlement_speaker(
    seats: &[SeatInfo; 4],
    in_flight_seats: &[bool; 4],
    idle_ai: &[(Seat, PersonaId)],
    settlement: &HandSettlement,
    event_seq: u64,
) -> Option<(Seat, PersonaId)> {
    if let HandOutcome::Win {
        winners, discarder, ..
    } = &settlement.outcome
    {
        // 1. Prioritize an idle AI winner (especially RobKongHu / highest fan).
        if let Some(best_ai_winner) = winners
            .iter()
            .filter_map(|w| {
                find_idle_ai_persona(seats, in_flight_seats, w.seat).map(|pair| (pair, w.total_fan))
            })
            .max_by_key(|&(_, fan)| fan)
        {
            return Some(best_ai_winner.0);
        }
        // 2. Next prioritize an idle AI that dealt in (or got robbed of a kong).
        if let Some(disc) = discarder {
            if let Some(found) = find_idle_ai_persona(seats, in_flight_seats, *disc) {
                return Some(found);
            }
        }
    }
    pick_deterministic_ai(idle_ai, event_seq)
}

#[cfg(test)]
mod tests {
    use mahjong_ai::analyze_hand_facts;
    use mahjong_core::{
        HandOutcome, HandSettlement, MeldView, PlayerHandView, PlayerView, PublicPhase,
        PublicSeatView, RevealedHand, RevealedMeld, Ruleset, ScoreItem, Seat, Tile,
        TuidaohuRuleset, TurnView, WinKind, Wind, WinnerSettlement,
    };

    use super::{
        build_ai_discard_checkin_prompt, build_chain_reply_prompt, build_hand_settled_prompt,
        build_meld_declared_prompt, build_system_prompt, compose_scene_and_six_blocks,
        format_hand_facts_zh, format_tile_zh, BanterChainContext, BanterChainTrigger,
        BanterSessionState, ChainUtterance,
    };
    use crate::banter_context::{seat_direction_and_name, MatchBanterContext, SixBlockInput};
    use crate::persona::{PersonaId, PersonaMood};
    use crate::persona_memory::PersonaMemoryStore;
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
                nickname: "大川".to_string(),
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
                nickname: "小晚".to_string(),
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
                nickname: "老陈".to_string(),
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
    fn system_prompt_enforces_humanized_short_phrase_contract_without_dealer_bias() {
        for pid in PersonaId::ALL {
            let prompt = build_system_prompt(pid);
            assert!(prompt.contains(pid.display_name()));
            for mood in [
                "Smug", "Mocking", "Tilted", "Nervous", "Plotting", "Deadpan",
            ] {
                assert!(prompt.contains(mood), "missing mood anchor {mood}");
            }
            assert!(prompt.contains("<think>"));
            assert!(prompt.contains("2 到 18 字"));
            assert!(prompt.contains("4 到 14 字"));
            assert!(!prompt.contains("10 到 26 字"));
            assert!(!prompt.contains("发牌员"));
            assert!(prompt.contains("speak: false"));
            assert!(prompt.contains("不要在句首硬加对方姓名"));
            assert!(prompt.contains("15% 到 25%"));
            assert!(prompt.contains("严禁捏造"));
            assert!(prompt.contains("预计算手牌事实"));
        }
    }

    #[test]
    fn mid_hand_prompts_include_six_blocks_and_hide_opponent_concealed_kong_tile() {
        let seats = sample_seats();
        // 13-tile tenpai hand: 123m 456m 789p 23s 55p (waits on 1s and 4s, 门前清)
        let closed = vec![
            Tile::new(0).unwrap(),
            Tile::new(1).unwrap(),
            Tile::new(2).unwrap(),
            Tile::new(3).unwrap(),
            Tile::new(4).unwrap(),
            Tile::new(5).unwrap(),
            Tile::new(24).unwrap(),
            Tile::new(25).unwrap(),
            Tile::new(26).unwrap(),
            Tile::new(10).unwrap(),
            Tile::new(11).unwrap(),
            Tile::new(22).unwrap(),
            Tile::new(22).unwrap(),
        ];
        let view = dummy_view(Seat::South, closed, None);
        let facts = analyze_hand_facts(&view, None);
        let facts_str = format_hand_facts_zh(&facts);
        assert!(facts_str.contains("已听牌"));
        assert!(facts_str.contains("一条(余4张/1番)"));
        assert!(facts_str.contains("四条(余4张/1番)"));

        let banter_ctx = MatchBanterContext::default();
        let memory = PersonaMemoryStore::in_memory();
        let checkin_prompt = compose_scene_and_six_blocks(
            build_ai_discard_checkin_prompt(Tile::new(27).unwrap()),
            &banter_ctx,
            &SixBlockInput {
                speaker: Seat::South,
                persona_id: PersonaId::QuickHothead,
                seats: &seats,
                view: &view,
                facts: Some(&facts),
                hands_revealed: false,
                persona_memory: &memory,
                local_time_zh: "周六深夜（周末）",
            },
        );
        assert!(checkin_prompt.contains("自家出牌巡检"));
        assert!(checkin_prompt.contains("预计算手牌事实"));
        assert!(checkin_prompt.contains("一条(余4张/1番)"));
        assert!(checkin_prompt.contains("严禁臆测他家暗牌"));
        assert!(checkin_prompt.contains("【1. 本场对局战况编年史】"));
        assert!(checkin_prompt.contains("【6. 现实时间语境】"));

        // Opponent ConcealedKong with tile: None must not expose any tile name.
        let hidden_kong = MeldView::ConcealedKong {
            tile: None,
            seq: 10,
        };
        let meld_prompt = compose_scene_and_six_blocks(
            build_meld_declared_prompt(&seats, Seat::South, Seat::East, &hidden_kong, 60),
            &banter_ctx,
            &SixBlockInput {
                speaker: Seat::South,
                persona_id: PersonaId::QuickHothead,
                seats: &seats,
                view: &view,
                facts: Some(&facts),
                hands_revealed: false,
                persona_memory: &memory,
                local_time_zh: "周六深夜（周末）",
            },
        );
        assert!(meld_prompt.contains("暗杠了一手（牌面保密未知）"));
        assert!(!meld_prompt.contains("发财"));

        // Mid-hand chain reply includes prior speaker text + own facts without opponent hidden tiles
        // AND formats all seats with separated direction + name from the follow-up speaker's perspective (West).
        let chain = BanterChainContext {
            trigger: BanterChainTrigger::MeldDeclared {
                actor: Seat::South,
                meld: MeldView::Pon {
                    tile: Tile::new(0).unwrap(),
                    from_seat: Seat::East,
                    seq: 8,
                },
                wall_remaining: 54,
            },
            hands_revealed: false,
            replies_emitted: 0,
            max_replies: 2,
            spoken_seats: vec![Seat::South],
            history: Vec::new(),
        };
        let history = vec![ChainUtterance {
            seat: Seat::South,
            mood: PersonaMood::Smug,
            text: "这碰下来稳赢了！".to_string(),
        }];
        let reply_prompt = compose_scene_and_six_blocks(
            build_chain_reply_prompt(&seats, Seat::West, &history, &chain),
            &banter_ctx,
            &SixBlockInput {
                speaker: Seat::West,
                persona_id: PersonaId::TeaSarcasm,
                seats: &seats,
                view: &view,
                facts: Some(&facts),
                hands_revealed: false,
                persona_memory: &memory,
                local_time_zh: "周六深夜（周末）",
            },
        );
        assert!(reply_prompt.contains("这碰下来稳赢了！"));
        assert!(reply_prompt.contains("上家 大川"));
        assert!(reply_prompt.contains("对门 小王"));
        assert!(!reply_prompt.contains("大川（自家）"));
        assert!(!reply_prompt.contains("大川（上家）"));
        assert!(!reply_prompt.contains("（自己）"));
        assert!(reply_prompt.contains("预计算手牌事实"));
        assert!(reply_prompt.contains("严禁臆测他家暗牌"));
    }

    #[test]
    fn seat_direction_and_settlement_chain_use_separated_labels_and_replier_perspective() {
        let seats = sample_seats();
        assert_eq!(
            seat_direction_and_name(&seats, Seat::South, Seat::South),
            "自家 大川"
        );
        assert_eq!(
            seat_direction_and_name(&seats, Seat::West, Seat::South),
            "上家 大川"
        );
        assert_eq!(
            seat_direction_and_name(&seats, Seat::West, Seat::West),
            "自家 小晚"
        );

        let settlement = HandSettlement {
            wind_round: Wind::East,
            hand_number: 1,
            dealer_streak: 0,
            dealer: Seat::East,
            outcome: HandOutcome::Win {
                winners: vec![WinnerSettlement {
                    seat: Seat::South,
                    win_kind: WinKind::Ron,
                    items: vec![ScoreItem {
                        code: "menqing".to_string(),
                        label: "门前清".to_string(),
                        fan: 1,
                    }],
                    total_fan: 1,
                    base_points: 2,
                }],
                discarder: Some(Seat::East),
                winning_tile: Tile::new(0).unwrap(),
            },
            kong_transfers: Vec::new(),
            score_deltas: [-2, 2, 0, 0],
            new_scores: [-2, 2, 0, 0],
            revealed_hands: Seat::ALL.map(|seat| RevealedHand {
                seat,
                closed_tiles: vec![Tile::new(seat.index() as u8).unwrap()],
                drawn_tile: None,
                melds: Vec::new(),
            }),
        };

        let chain = BanterChainContext {
            trigger: BanterChainTrigger::HandSettled(Box::new(settlement)),
            hands_revealed: true,
            replies_emitted: 0,
            max_replies: 2,
            spoken_seats: vec![Seat::South],
            history: Vec::new(),
        };
        let history = vec![ChainUtterance {
            seat: Seat::South,
            mood: PersonaMood::Smug,
            text: "点炮舒服不？".to_string(),
        }];
        let west_reply = build_chain_reply_prompt(&seats, Seat::West, &history, &chain);
        assert!(west_reply.contains("自家 小晚"));
        assert!(west_reply.contains("上家 大川"));
        assert!(west_reply.contains("对门 小王"));
        assert!(!west_reply.contains("大川（自家）"));
        assert!(!west_reply.contains("小王（自家）"));
        assert!(!west_reply.contains("（自己）"));
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
        assert!(prompt.contains("小晚"));
        assert!(prompt.contains("老陈"));
    }

    #[test]
    fn single_in_flight_slot_prevents_overlap_and_ignores_stale_job_release() {
        let mut state = BanterSessionState::new(true);
        let id1 = state
            .try_claim_seat(Seat::South)
            .expect("first claim succeeds");
        assert!(state.in_flight_flags()[Seat::South.index()]);
        assert_eq!(state.try_claim_seat(Seat::South), None);

        // Clearing seat (e.g. next hand) lets a new job claim the slot; stale id1 release must not clear id2.
        state.clear_all_in_flight();
        let id2 = state
            .try_claim_seat(Seat::South)
            .expect("new claim after hand transition succeeds");
        assert_ne!(id1, id2);
        state.release_seat(Seat::South, id1);
        assert!(
            state.in_flight_flags()[Seat::South.index()],
            "stale release must never clear newer in-flight slot"
        );
        state.release_seat(Seat::South, id2);
        assert!(!state.in_flight_flags()[Seat::South.index()]);
    }
}
