//! Presentation-only read-only match context (`MatchBanterContext`) and 6-block single-seat
//! prompt assembly for autonomous LLM banter (`ADR-0015`).
//!
//! Strictly consumes only single-seat projected views (`PlayerView`), public events, settled
//! hand summaries, cross-hand public transcript (`牌桌对话流`), scoped `PersonaMemory`, and host
//! local time. Never accesses or leaks any opponent concealed tiles or opponent `ConcealedKong`
//! tile faces.

use std::collections::VecDeque;
use std::fmt::Write as _;

use mahjong_ai::{analyze_hand_facts, HandFacts, HandProgression};
use mahjong_core::{
    HandOutcome, HandSettlement, MatchMode, MeldView, PlayerEventKind, PlayerView, PublicPhase,
    RecordedEvent, Seat, Suit, Tile, WinKind, Wind,
};

use crate::banter::{format_hand_facts_zh, format_mood_zh, format_own_hand, format_tile_zh};
use crate::persona::{PersonaId, PersonaMood};
use crate::persona_memory::PersonaMemoryStore;
use crate::protocol::{QuickEmote, SeatInfo, SeatKind};
use crate::session::ActiveMatch;

/// Rolling capacity for the cross-hand match transcript (`牌桌对话流`, spec target `15–20`).
pub const TABLE_TRANSCRIPT_CAPACITY: usize = 20;

/// Maximum settled hands rendered verbatim in Block 1 chronicle to stay concise on long FreeTables.
const MAX_CHRONICLE_HANDS_IN_PROMPT: usize = 12;

/// Maximum recent per-turn trajectory steps kept for each AI seat within a single hand.
const MAX_TRAJECTORY_STEPS: usize = 6;

/// Relative table direction from `speaker`'s first-person perspective (`GLOSSARY.md`).
#[must_use]
pub fn relative_relation_zh(speaker: Seat, target: Seat) -> &'static str {
    let diff = (target.index() + 4 - speaker.index()) % 4;
    match diff {
        0 => "自家",
        1 => "下家",
        2 => "对门",
        _ => "上家",
    }
}

/// Returns a seat's natural display name without appending mechanical direction brackets.
#[must_use]
pub fn seat_display_name(seats: &[SeatInfo; 4], target: Seat) -> String {
    let name = seats[target.index()].nickname.trim();
    if name.is_empty() {
        format!("玩家{}", target.index() + 1)
    } else {
        name.to_string()
    }
}

/// Formats a seat as separated direction and natural name (e.g. `上家 小王`, `自家 大川`),
/// avoiding rigid bracketed identity tags that small models copy verbatim.
#[must_use]
pub fn seat_direction_and_name(seats: &[SeatInfo; 4], speaker: Seat, target: Seat) -> String {
    format!(
        "{} {}",
        relative_relation_zh(speaker, target),
        seat_display_name(seats, target)
    )
}

/// Formats a Chinese wind + 1-based hand label (e.g. `东一局`, `南三局`).
#[must_use]
pub fn format_wind_hand_zh(wind: Wind, hand_number: u8) -> String {
    let wind_zh = match wind {
        Wind::East => "东",
        Wind::South => "南",
        Wind::West => "西",
        Wind::North => "北",
    };
    let num_zh = match hand_number {
        1 => "一",
        2 => "二",
        3 => "三",
        4 => "四",
        _ => return format!("{wind_zh}{hand_number}局"),
    };
    format!("{wind_zh}{num_zh}局")
}

/// Formats a concise table anchor from a projected view (e.g. `东一局·余52张` or `东一局结算`).
#[must_use]
pub fn format_view_anchor(view: &PlayerView) -> String {
    let hand_label = format_wind_hand_zh(view.turn.wind_round, view.turn.hand_number);
    match view.phase {
        PublicPhase::Playing => format!("{hand_label}·余{}张", view.wall_remaining),
        PublicPhase::BetweenHands | PublicPhase::MatchFinished => format!("{hand_label}结算"),
    }
}

/// Formats a host weekday (`0` = Sunday .. `6` = Saturday) and 24-hour clock (`0..=23`)
/// into a vivid Chinese time-of-day description for natural casual banter.
#[must_use]
pub fn format_local_time_zh(weekday_sun0: u8, hour: u8) -> String {
    let weekday = match weekday_sun0 % 7 {
        0 => "周日",
        1 => "周一",
        2 => "周二",
        3 => "周三",
        4 => "周四",
        5 => "周五",
        _ => "周六",
    };
    let period = match hour % 24 {
        0..=4 | 23 => "深夜",
        5..=7 => "清晨",
        8..=10 => "上午",
        11..=13 => "中午",
        14..=16 => "下午",
        17..=18 => "傍晚",
        _ => "晚上",
    };
    let lifestyle = match (weekday_sun0 % 7, hour % 24) {
        (0, 19..=23) => "周末收尾，明早工作日",
        (0 | 6, _) => "周末",
        (5, 17..=23) => "周五晚，快周末了",
        _ => "工作日",
    };
    format!("{weekday}{period}（{lifestyle}）")
}

/// Reads the host machine's current local weekday and time-of-day as Chinese context.
#[must_use]
pub fn current_local_time_zh() -> String {
    let (wday, hour) = current_local_weekday_and_hour();
    format_local_time_zh(wday, hour)
}

#[cfg(unix)]
fn current_local_weekday_and_hour() -> (u8, u8) {
    use std::mem::MaybeUninit;
    // SAFETY: `libc::time` accepts null and returns current Unix timestamp; `libc::localtime_r`
    // writes into the provided stack `tm` buffer and returns null only on overflow.
    unsafe {
        let now: libc::time_t = libc::time(std::ptr::null_mut());
        let mut tm = MaybeUninit::<libc::tm>::zeroed();
        let ptr = libc::localtime_r(&now, tm.as_mut_ptr());
        if ptr.is_null() {
            return (0, 12);
        }
        let tm = tm.assume_init();
        let wday = u8::try_from(tm.tm_wday.rem_euclid(7)).unwrap_or(0);
        let hour = u8::try_from(tm.tm_hour.rem_euclid(24)).unwrap_or(12);
        (wday, hour)
    }
}

#[cfg(not(unix))]
fn current_local_weekday_and_hour() -> (u8, u8) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    // 1970-01-01 was Thursday (weekday 4 when Sunday = 0).
    let wday = ((days + 4) % 7) as u8;
    let hour = ((secs / 3600) % 24) as u8;
    (wday, hour)
}

/// Concise winner summary stored in [`MatchChronicle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChronicleWinner {
    pub seat: Seat,
    pub win_kind: WinKind,
    pub total_fan: u8,
    pub main_fans: Vec<String>,
}

/// Settled outcome recorded in [`MatchChronicle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChronicleOutcome {
    DrawGame,
    Win {
        winners: Vec<ChronicleWinner>,
        discarder: Option<Seat>,
        winning_tile: Tile,
    },
}

/// One settled hand summary in the match chronicle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledHandSummary {
    pub wind_round: Wind,
    pub hand_number: u8,
    pub dealer: Seat,
    pub dealer_streak: u32,
    pub outcome: ChronicleOutcome,
    pub score_deltas: [i32; 4],
}

/// Block 1 state: match-scoped chronicle of prior hands and per-seat win/loss streaks.
#[derive(Debug, Clone, Default)]
pub struct MatchChronicle {
    settled_hands: Vec<SettledHandSummary>,
    win_streaks: [u32; 4],
    loss_streaks: [u32; 4],
}

impl MatchChronicle {
    /// Records one completed hand into the match chronicle and updates win/loss streaks.
    pub fn record_hand_settled(&mut self, settlement: &HandSettlement) {
        let mut won_this_hand = [false; 4];
        let outcome = match &settlement.outcome {
            HandOutcome::DrawGame => ChronicleOutcome::DrawGame,
            HandOutcome::Win {
                winners,
                discarder,
                winning_tile,
            } => {
                let mut recorded_winners = Vec::with_capacity(winners.len());
                for w in winners {
                    won_this_hand[w.seat.index()] = true;
                    let main_fans: Vec<String> = if w.items.is_empty() {
                        vec!["基本胡".to_string()]
                    } else {
                        w.items.iter().take(4).map(|i| i.label.clone()).collect()
                    };
                    recorded_winners.push(ChronicleWinner {
                        seat: w.seat,
                        win_kind: w.win_kind,
                        total_fan: w.total_fan,
                        main_fans,
                    });
                }
                ChronicleOutcome::Win {
                    winners: recorded_winners,
                    discarder: *discarder,
                    winning_tile: *winning_tile,
                }
            }
        };

        for seat in Seat::ALL {
            let idx = seat.index();
            if won_this_hand[idx] {
                self.win_streaks[idx] = self.win_streaks[idx].saturating_add(1);
                self.loss_streaks[idx] = 0;
            } else {
                self.win_streaks[idx] = 0;
                if settlement.score_deltas[idx] < 0 {
                    self.loss_streaks[idx] = self.loss_streaks[idx].saturating_add(1);
                } else {
                    self.loss_streaks[idx] = 0;
                }
            }
        }

        self.settled_hands.push(SettledHandSummary {
            wind_round: settlement.wind_round,
            hand_number: settlement.hand_number,
            dealer: settlement.dealer,
            dealer_streak: settlement.dealer_streak,
            outcome,
            score_deltas: settlement.score_deltas,
        });
    }

    /// Number of settled hands recorded so far in this match.
    #[must_use]
    pub fn settled_count(&self) -> usize {
        self.settled_hands.len()
    }

    /// Formats Block 1 (`本场对局战况编年史`) strictly from `speaker`'s perspective.
    #[must_use]
    pub fn format_block(&self, view: &PlayerView, seats: &[SeatInfo; 4], speaker: Seat) -> String {
        let mode_zh = match view.mode {
            MatchMode::EastOnly => "东风战",
            MatchMode::FreeTable => "自由桌",
        };
        let hand_zh = format_wind_hand_zh(view.turn.wind_round, view.turn.hand_number);
        let dealer_str = seat_direction_and_name(seats, speaker, view.turn.dealer);
        let streak_note = if view.turn.dealer_streak > 0 {
            format!("，连庄 {} 次", view.turn.dealer_streak)
        } else {
            String::new()
        };

        let mut out = format!(
            "【1. 本场对局战况编年史】\n当前进度：{mode_zh} {hand_zh}（庄家：{dealer_str}{streak_note}，本场已结 {} 小局）",
            self.settled_hands.len()
        );

        // Four-seat scores & rankings sorted descending by score.
        let mut ranked: Vec<(Seat, i32)> = Seat::ALL
            .into_iter()
            .map(|s| (s, view.seats[s.index()].score))
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.index().cmp(&b.0.index())));
        let rank_parts: Vec<String> = ranked
            .iter()
            .enumerate()
            .map(|(idx, &(seat, score))| {
                format!(
                    "第{}名 {} {:+}分",
                    idx + 1,
                    seat_direction_and_name(seats, speaker, seat),
                    score
                )
            })
            .collect();
        let _ = write!(out, "\n四家总分与排位：{}", rank_parts.join("；"));

        // Streak callouts if anyone is on a hot or cold streak.
        let mut streak_notes = Vec::new();
        for seat in Seat::ALL {
            let idx = seat.index();
            if self.win_streaks[idx] >= 2 {
                streak_notes.push(format!(
                    "{} 连胡 {} 小局",
                    seat_direction_and_name(seats, speaker, seat),
                    self.win_streaks[idx]
                ));
            } else if self.loss_streaks[idx] >= 2 {
                streak_notes.push(format!(
                    "{} 连失分 {} 小局",
                    seat_direction_and_name(seats, speaker, seat),
                    self.loss_streaks[idx]
                ));
            }
        }
        if !streak_notes.is_empty() {
            let _ = write!(out, "\n连胜连败走势：{}", streak_notes.join("；"));
        }

        // Concise history of prior settled hands.
        if self.settled_hands.is_empty() {
            out.push_str("\n过往战报：本小局为开桌首小局，尚无已结算战绩。");
        } else {
            out.push_str("\n过往战报：");
            let total = self.settled_hands.len();
            let start = total.saturating_sub(MAX_CHRONICLE_HANDS_IN_PROMPT);
            if start > 0 {
                let _ = write!(out, "\n- （更早 {} 小局已略去）", start);
            }
            for (offset, hand) in self.settled_hands[start..].iter().enumerate() {
                let seq_no = start + offset + 1;
                let h_label = format_wind_hand_zh(hand.wind_round, hand.hand_number);
                let lianzhuang = if hand.dealer_streak > 0 {
                    format!("·连庄{}", hand.dealer_streak)
                } else {
                    String::new()
                };
                match &hand.outcome {
                    ChronicleOutcome::DrawGame => {
                        let _ = write!(out, "\n- 第{seq_no}小局({h_label}{lianzhuang})：荒庄");
                    }
                    ChronicleOutcome::Win {
                        winners,
                        discarder,
                        winning_tile,
                    } => {
                        let tile_zh = format_tile_zh(*winning_tile);
                        let winner_parts: Vec<String> = winners
                            .iter()
                            .map(|w| {
                                let wk = match w.win_kind {
                                    WinKind::Tsumo => "自摸",
                                    WinKind::Ron => "点炮胡",
                                    WinKind::RobKongHu => "抢杠胡",
                                };
                                format!(
                                    "{} {wk} {tile_zh}（{}番：{}）",
                                    seat_direction_and_name(seats, speaker, w.seat),
                                    w.total_fan,
                                    w.main_fans.join("+")
                                )
                            })
                            .collect();
                        let disc_note = discarder.map_or_else(String::new, |d| {
                            format!("，点炮者：{}", seat_direction_and_name(seats, speaker, d))
                        });
                        let dealer_won = winners.iter().any(|w| w.seat == hand.dealer);
                        let dealer_keep = if dealer_won { "，庄家连庄" } else { "" };
                        let _ = write!(
                            out,
                            "\n- 第{seq_no}小局({h_label}{lianzhuang})：{}{disc_note}{dealer_keep}",
                            winner_parts.join("；")
                        );
                    }
                }
            }
        }

        out
    }
}

/// Formats Block 2 (`本小局公开动态与牌河画像`) strictly from a single speaker's [`PlayerView`].
///
/// Privacy guarantee: opponent `MeldView::ConcealedKong` is unconditionally masked even if
/// a caller passed a non-empty `tile` field.
#[must_use]
pub fn format_public_table_portrait(
    view: &PlayerView,
    seats: &[SeatInfo; 4],
    speaker: Seat,
) -> String {
    let stage = match view.wall_remaining {
        56.. => "开局初盘",
        28..=55 => "中盘阶段",
        12..=27 => "中后盘",
        _ => "尾盘紧张",
    };
    let mut out = format!(
        "【2. 本小局公开动态与牌河画像】\n牌墙进度：剩余 {} 张（{stage}）",
        view.wall_remaining
    );

    // Order seats starting from speaker (`自家`, `下家`, `对门`, `上家`).
    let relative_seats = relative_seat_order(speaker);

    // 1. Public melds across all 4 seats (strict opponent ConcealedKong masking).
    let mut meld_lines = Vec::new();
    for seat in relative_seats {
        let melds = &view.seats[seat.index()].melds;
        if melds.is_empty() {
            continue;
        }
        let items: Vec<String> = melds
            .iter()
            .map(|meld| format_meld_view_for_portrait(seats, speaker, seat, meld))
            .collect();
        meld_lines.push(format!(
            "{}：{}",
            seat_direction_and_name(seats, speaker, seat),
            items.join("、")
        ));
    }
    if meld_lines.is_empty() {
        out.push_str("\n公开鸣牌：全桌暂无人鸣牌。");
    } else {
        let _ = write!(out, "\n公开鸣牌：{}", meld_lines.join("；"));
    }

    // 2. Per-seat discard cadence (摸切/手切) and river suit concentration / void cues.
    out.push_str("\n各家牌河与节奏：");
    for seat in relative_seats {
        let river = &view.seats[seat.index()].river;
        let who = seat_direction_and_name(seats, speaker, seat);
        if river.is_empty() {
            let _ = write!(out, "\n- {who}：暂未舍牌");
            continue;
        }

        // Trailing consecutive 摸切 or 手切 cadence.
        let last_tsumogiri = river.last().is_some_and(|d| d.is_tsumogiri);
        let streak = river
            .iter()
            .rev()
            .take_while(|d| d.is_tsumogiri == last_tsumogiri)
            .count();
        let cadence = if streak >= 2 {
            if last_tsumogiri {
                format!("连摸切{streak}手")
            } else {
                format!("连手切{streak}手")
            }
        } else if last_tsumogiri {
            "上巡摸切".to_string()
        } else {
            "上巡手切".to_string()
        };

        // Recent up to 5 discards.
        let recent_start = river.len().saturating_sub(5);
        let recent_tiles: Vec<String> = river[recent_start..]
            .iter()
            .map(|d| {
                let tag = if d.is_tsumogiri { "摸" } else { "手" };
                format!("{}({tag})", format_tile_zh(d.tile))
            })
            .collect();

        // Suit counts and apparent void / concentration cues.
        let mut man = 0usize;
        let mut bamboo = 0usize;
        let mut dot = 0usize;
        let mut honor = 0usize;
        for entry in river {
            match entry.tile.suit() {
                Suit::Man => man += 1,
                Suit::Bamboo => bamboo += 1,
                Suit::Dot => dot += 1,
                Suit::Wind | Suit::Dragon => honor += 1,
            }
        }
        let mut tendencies = Vec::new();
        if river.len() >= 4 {
            let suit_counts = [("万", man), ("条", bamboo), ("筒", dot)];
            for &(label, count) in &suit_counts {
                if count >= 3 && count * 2 >= river.len() {
                    tendencies.push(format!("集中舍{label}（似缺{label}）"));
                }
            }
            if river.len() >= 6 {
                for &(label, count) in &suit_counts {
                    let other_two_active = suit_counts
                        .iter()
                        .filter(|&&(l, _)| l != label)
                        .all(|&(_, c)| c >= 2);
                    if count == 0 && other_two_active {
                        tendencies.push(format!("牌河未见{label}"));
                    }
                }
            }
        }
        let suit_summary = if tendencies.is_empty() {
            format!("万{man}/条{bamboo}/筒{dot}/字{honor}")
        } else {
            format!(
                "万{man}/条{bamboo}/筒{dot}/字{honor}，{}",
                tendencies.join("、")
            )
        };

        let _ = write!(
            out,
            "\n- {who}（共舍{}张，{cadence}，{suit_summary}）：近舍 {}",
            river.len(),
            recent_tiles.join("、")
        );
    }

    out
}

fn relative_seat_order(speaker: Seat) -> [Seat; 4] {
    let base = speaker.index();
    [
        Seat::ALL[base],
        Seat::ALL[(base + 1) % 4],
        Seat::ALL[(base + 2) % 4],
        Seat::ALL[(base + 3) % 4],
    ]
}

fn format_meld_view_for_portrait(
    seats: &[SeatInfo; 4],
    speaker: Seat,
    owner: Seat,
    meld: &MeldView,
) -> String {
    match meld {
        MeldView::Pon {
            tile, from_seat, ..
        } => format!(
            "碰{}（取自{}）",
            format_tile_zh(*tile),
            seat_direction_and_name(seats, speaker, *from_seat)
        ),
        MeldView::ExposedKong {
            tile, from_seat, ..
        } => format!(
            "明杠{}（取自{}）",
            format_tile_zh(*tile),
            seat_direction_and_name(seats, speaker, *from_seat)
        ),
        MeldView::AddedKong { tile, .. } => format!("补杠{}", format_tile_zh(*tile)),
        MeldView::ConcealedKong { tile, .. } => match tile {
            Some(t) if owner == speaker => format!("暗杠{}", format_tile_zh(*t)),
            _ => "暗杠一手（牌面保密未知）".to_string(),
        },
        MeldView::Chi {
            called_tile,
            from_seat,
            ..
        } => format!(
            "吃{}（取自{}）",
            format_tile_zh(*called_tile),
            seat_direction_and_name(seats, speaker, *from_seat)
        ),
    }
}

/// Opening-hand quality snapshot captured at `HandStarted` for one AI seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpeningHandSnapshot {
    pub shanten: i8,
    pub ukeire: u16,
    pub directions: Vec<String>,
}

impl OpeningHandSnapshot {
    fn from_facts(facts: &HandFacts) -> Self {
        Self {
            shanten: facts.shanten,
            ukeire: facts.ukeire,
            directions: facts
                .potential_directions
                .iter()
                .map(|d| d.label_zh().to_string())
                .collect(),
        }
    }

    #[must_use]
    pub fn format_zh(&self) -> String {
        let quality = match self.shanten {
            ..=0 => "起手极顺（直接听牌/成胡）",
            1 => "起手很顺（1向听）",
            2 => "起手顺手（2向听）",
            3 => "起手一般（3向听）",
            4 => "起手偏散（4向听）",
            _ => "起手很碎（多向听）",
        };
        if self.directions.is_empty() {
            format!("{quality}，有效进张{}张", self.ukeire)
        } else {
            format!(
                "{quality}，有效进张{}张，初始大番苗头：{}",
                self.ukeire,
                self.directions.join("、")
            )
        }
    }
}

/// One turn's progression point in an AI seat's single-hand trajectory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrajectoryStep {
    pub turn_no: u32,
    pub shanten: i8,
    pub ukeire: u16,
    pub progression: HandProgression,
    pub stalled_streak: u8,
    pub discarded: Option<Tile>,
}

/// Block 3 state: single-hand opening snapshot & rolling trajectory for one AI seat.
#[derive(Debug, Clone, Default)]
pub struct SeatHandTrajectory {
    pub opening: Option<OpeningHandSnapshot>,
    pub last_facts: Option<HandFacts>,
    steps: VecDeque<TrajectoryStep>,
    turn_counter: u32,
}

impl SeatHandTrajectory {
    /// Captures the opening hand quality at `HandStarted`.
    pub fn reset_for_hand(&mut self, initial_facts: HandFacts) {
        self.opening = Some(OpeningHandSnapshot::from_facts(&initial_facts));
        self.last_facts = Some(initial_facts);
        self.steps.clear();
        self.turn_counter = 0;
    }

    /// Clears all state (e.g. for human seats).
    pub fn clear(&mut self) {
        self.opening = None;
        self.last_facts = None;
        self.steps.clear();
        self.turn_counter = 0;
    }

    /// Records a post-discard or post-meld hand facts update.
    pub fn record_step(&mut self, facts: HandFacts, discarded: Option<Tile>) {
        self.turn_counter = self.turn_counter.saturating_add(1);
        let step = TrajectoryStep {
            turn_no: self.turn_counter,
            shanten: facts.shanten,
            ukeire: facts.ukeire,
            progression: facts.progression,
            stalled_streak: facts.stalled_streak,
            discarded,
        };
        if self.steps.len() >= MAX_TRAJECTORY_STEPS {
            self.steps.pop_front();
        }
        self.steps.push_back(step);
        self.last_facts = Some(facts);
    }

    /// Formats Block 3 (`自家手牌、预计算手牌事实与牌运轨迹`).
    #[must_use]
    pub fn format_block(
        &self,
        view: Option<&PlayerView>,
        facts: Option<&HandFacts>,
        current_mood: PersonaMood,
        hands_revealed: bool,
    ) -> String {
        let mut out = String::from("【3. 自家手牌、预计算手牌事实与牌运轨迹】");
        if !hands_revealed {
            if let Some(v) = view {
                let _ = write!(
                    out,
                    "\n自家手牌：{}（严禁臆测他家暗牌）",
                    format_own_hand(v)
                );
            }
            if let Some(f) = facts {
                let _ = write!(out, "\n预计算手牌事实：{}", format_hand_facts_zh(f));
            }
        }
        if let Some(opening) = &self.opening {
            let _ = write!(out, "\n本小局起手评估：{}", opening.format_zh());
        }
        if self.steps.is_empty() {
            out.push_str("\n近几巡牌运轨迹：本小局刚开打，尚未积累多巡进退记录。");
        } else {
            let step_strs: Vec<String> = self
                .steps
                .iter()
                .map(|s| {
                    let prog = match s.progression {
                        HandProgression::Initial => "初始",
                        HandProgression::CanWin => "已成胡",
                        HandProgression::EnteredTenpai => "刚听牌",
                        HandProgression::Improved => "提速",
                        HandProgression::Stalled if s.stalled_streak >= 2 => "连卡",
                        HandProgression::Stalled => "停滞",
                        HandProgression::Regressed => "变窄",
                    };
                    let shanten_str = if s.shanten <= 0 {
                        "听牌/成胡".to_string()
                    } else {
                        format!("{}向听", s.shanten)
                    };
                    let stall_note = if s.stalled_streak >= 2 {
                        format!("/连卡{}巡", s.stalled_streak)
                    } else {
                        String::new()
                    };
                    let disc_note = s
                        .discarded
                        .map(|t| format!("舍{}", format_tile_zh(t)))
                        .unwrap_or_default();
                    if disc_note.is_empty() {
                        format!(
                            "第{}巡 {shanten_str}(进张{},{prog}{stall_note})",
                            s.turn_no, s.ukeire
                        )
                    } else {
                        format!(
                            "第{}巡 {disc_note}→{shanten_str}(进张{},{prog}{stall_note})",
                            s.turn_no, s.ukeire
                        )
                    }
                })
                .collect();
            let _ = write!(out, "\n近几巡牌运轨迹：{}", step_strs.join(" → "));
        }
        let _ = write!(
            out,
            "\n自家当前牌友情绪：{}（{current_mood:?}）",
            format_mood_zh(current_mood)
        );
        out
    }
}

/// Kind of public entry stored in [`TableTranscript`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptEntryKind {
    Banter { mood: PersonaMood, text: String },
    QuickEmote { emote: QuickEmote, text: String },
}

/// One public banter line or quick emote recorded in the match-scoped [`TableTranscript`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    pub seat: Seat,
    pub speaker_name: String,
    pub kind: TranscriptEntryKind,
    pub anchor: String,
    pub global_discards: u32,
}

/// Block 4 state: match-scoped cross-hand rolling window of public `TableBanter` & `QuickEmote`.
#[derive(Debug, Clone, Default)]
pub struct TableTranscript {
    entries: VecDeque<TranscriptEntry>,
    global_discards: u32,
    seat_turns: [u32; 4],
    last_spoken_seat_turn: [Option<u32>; 4],
}

impl TableTranscript {
    /// Records one table discard so per-seat and table-wide turn counts stay exact.
    pub fn record_discard(&mut self, seat: Seat) {
        self.global_discards = self.global_discards.saturating_add(1);
        self.seat_turns[seat.index()] = self.seat_turns[seat.index()].saturating_add(1);
    }

    /// Appends a committed `TableBanter` (`speak == true`) and updates the speaker's last-spoken turn.
    pub fn record_banter(
        &mut self,
        seat: Seat,
        speaker_name: String,
        mood: PersonaMood,
        text: String,
        anchor: String,
    ) {
        self.push_entry(TranscriptEntry {
            seat,
            speaker_name,
            kind: TranscriptEntryKind::Banter { mood, text },
            anchor,
            global_discards: self.global_discards,
        });
        self.last_spoken_seat_turn[seat.index()] = Some(self.seat_turns[seat.index()]);
    }

    /// Appends a human `QuickEmoteBroadcast` to the rolling transcript.
    pub fn record_quick_emote(
        &mut self,
        seat: Seat,
        speaker_name: String,
        emote: QuickEmote,
        text: String,
        anchor: String,
    ) {
        self.push_entry(TranscriptEntry {
            seat,
            speaker_name,
            kind: TranscriptEntryKind::QuickEmote { emote, text },
            anchor,
            global_discards: self.global_discards,
        });
    }

    fn push_entry(&mut self, entry: TranscriptEntry) {
        if self.entries.len() >= TABLE_TRANSCRIPT_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Returns how many of `seat`'s turns have elapsed since `seat` last spoke, or `None` if never.
    #[must_use]
    pub fn turns_since_last_spoke(&self, seat: Seat) -> Option<u32> {
        self.last_spoken_seat_turn[seat.index()]
            .map(|prev| self.seat_turns[seat.index()].saturating_sub(prev))
    }

    /// Returns the retained entries (oldest first, newest last).
    #[must_use]
    pub fn entries(&self) -> &VecDeque<TranscriptEntry> {
        &self.entries
    }

    /// Formats Block 4 (`跨小局牌桌对话流`) strictly from `speaker`'s first-person perspective.
    #[must_use]
    pub fn format_block(&self, seats: &[SeatInfo; 4], speaker: Seat) -> String {
        let mut out = String::from("【4. 跨小局牌桌对话流（近期公开台词与快捷互动）】");
        match self.turns_since_last_spoke(speaker) {
            None => out.push_str("\n发言间隔提示：你本场对局还没开过口。"),
            Some(0) => {
                out.push_str(
                    "\n发言间隔提示：你距离上次开口已过 0 巡（刚说过话不久，没新梗别硬接）。",
                );
            }
            Some(turns) => {
                let _ = write!(out, "\n发言间隔提示：你距离上次开口已过 {turns} 巡。");
            }
        }

        if self.entries.is_empty() {
            out.push_str("\n近期对话：本场暂无人发言或发送快捷互动。");
            return out;
        }

        out.push_str("\n近期对话（按时间先后，别复读刚说过的梗，可顺茬接话或打脸之前的 Flag）：");
        for entry in &self.entries {
            let diff_discards = self.global_discards.saturating_sub(entry.global_discards);
            let age = if diff_discards == 0 {
                "刚才".to_string()
            } else if diff_discards < 4 {
                "不到1巡前".to_string()
            } else {
                format!("{}巡前", diff_discards / 4)
            };
            let rel = relative_relation_zh(speaker, entry.seat);
            let name = if entry.speaker_name.trim().is_empty() {
                seat_display_name(seats, entry.seat)
            } else {
                entry.speaker_name.clone()
            };
            match &entry.kind {
                TranscriptEntryKind::Banter { mood, text } => {
                    let _ = write!(
                        out,
                        "\n- [{}｜{age}] {rel} {name}（{}）：「{text}」",
                        entry.anchor,
                        format_mood_zh(*mood)
                    );
                }
                TranscriptEntryKind::QuickEmote { text, .. } => {
                    let _ = write!(
                        out,
                        "\n- [{}｜{age}] {rel} {name} 发了快捷互动：「{text}」",
                        entry.anchor
                    );
                }
            }
        }

        out
    }
}

/// Cohesive presentation-only match state owned by `ActiveMatchState`, persisting across hands
/// within a match and cleanly dropped/reset on new match start or disband.
#[derive(Debug, Clone, Default)]
pub struct MatchBanterContext {
    pub chronicle: MatchChronicle,
    pub transcript: TableTranscript,
    pub seat_trajectories: [SeatHandTrajectory; 4],
}

/// Inputs required to format all 6 context blocks for one speaking AI seat.
pub(crate) struct SixBlockInput<'a> {
    pub speaker: Seat,
    pub persona_id: PersonaId,
    pub seats: &'a [SeatInfo; 4],
    pub view: &'a PlayerView,
    pub facts: Option<&'a HandFacts>,
    pub hands_revealed: bool,
    pub persona_memory: &'a PersonaMemoryStore,
    pub local_time_zh: &'a str,
}

impl MatchBanterContext {
    /// Creates a fresh match presentation context initialized with opening-hand snapshots for all AI seats.
    #[must_use]
    pub(crate) fn new(game: &ActiveMatch, seats: &[SeatInfo; 4]) -> Self {
        let mut ctx = Self::default();
        ctx.reset_for_new_hand(game, seats);
        ctx
    }

    /// Resets per-hand trajectories for a newly dealt hand while preserving `chronicle` and `transcript`.
    pub(crate) fn reset_for_new_hand(&mut self, game: &ActiveMatch, seats: &[SeatInfo; 4]) {
        for seat in Seat::ALL {
            let info = &seats[seat.index()];
            if info.kind == SeatKind::Ai && info.persona_id.is_some() {
                let view = game.project_view(seat);
                let facts = analyze_hand_facts(&view, None);
                self.seat_trajectories[seat.index()].reset_for_hand(facts);
            } else {
                self.seat_trajectories[seat.index()].clear();
            }
        }
    }

    /// Returns the previous [`HandFacts`] snapshot for `seat` in the current hand, if any.
    #[must_use]
    pub fn last_hand_facts(&self, seat: Seat) -> Option<&HandFacts> {
        self.seat_trajectories[seat.index()].last_facts.as_ref()
    }

    /// Records a hand settlement into the match chronicle.
    pub fn record_hand_settled(&mut self, settlement: &HandSettlement) {
        self.chronicle.record_hand_settled(settlement);
    }

    /// Records a discard into the transcript turn tracker.
    pub fn record_discard(&mut self, seat: Seat) {
        self.transcript.record_discard(seat);
    }

    /// Records an AI seat's updated hand facts & trajectory step.
    pub fn record_ai_hand_step(&mut self, seat: Seat, facts: HandFacts, discarded: Option<Tile>) {
        self.seat_trajectories[seat.index()].record_step(facts, discarded);
    }

    /// Records a committed `TableBanter` (`speak == true`) into the rolling match transcript.
    pub fn record_banter(
        &mut self,
        seat: Seat,
        speaker_name: String,
        mood: PersonaMood,
        text: String,
        anchor: String,
    ) {
        self.transcript
            .record_banter(seat, speaker_name, mood, text, anchor);
    }

    /// Records a `QuickEmoteBroadcast` into the rolling match transcript.
    pub fn record_quick_emote(
        &mut self,
        seat: Seat,
        speaker_name: String,
        emote: QuickEmote,
        text: String,
        anchor: String,
    ) {
        self.transcript
            .record_quick_emote(seat, speaker_name, emote, text, anchor);
    }

    /// Updates presentation-only trajectories and discard turn counters from an emitted event batch.
    ///
    /// - `HandStarted`: resets per-hand trajectories while preserving `chronicle` and `transcript`.
    /// - `TileDiscarded`: advances global & per-seat discard counters, and updates AI trajectory.
    /// - `MeldDeclared`: updates AI actor's trajectory silently.
    pub(crate) fn observe_events(
        &mut self,
        game: &ActiveMatch,
        seats: &[SeatInfo; 4],
        emitted: &[RecordedEvent],
    ) {
        for event in emitted {
            let projected = game.project_event(event, Seat::East);
            match projected.kind {
                PlayerEventKind::HandStarted { .. } => {
                    self.reset_for_new_hand(game, seats);
                }
                PlayerEventKind::TileDiscarded { seat, tile, .. } => {
                    self.record_discard(seat);
                    let info = &seats[seat.index()];
                    if info.kind == SeatKind::Ai && info.persona_id.is_some() {
                        let view = game.project_view(seat);
                        let facts = analyze_hand_facts(&view, self.last_hand_facts(seat));
                        self.record_ai_hand_step(seat, facts, Some(tile));
                    }
                }
                PlayerEventKind::MeldDeclared { seat: actor, .. } => {
                    let info = &seats[actor.index()];
                    if info.kind == SeatKind::Ai && info.persona_id.is_some() {
                        let view = game.project_view(actor);
                        let facts = analyze_hand_facts(&view, self.last_hand_facts(actor));
                        self.record_ai_hand_step(actor, facts, None);
                    }
                }
                _ => {}
            }
        }
    }

    /// Formats all 6 single-seat context blocks strictly from `input.speaker`'s perspective.
    #[must_use]
    pub(crate) fn format_six_blocks(&self, input: &SixBlockInput<'_>) -> String {
        let block1 = self
            .chronicle
            .format_block(input.view, input.seats, input.speaker);
        let block2 = format_public_table_portrait(input.view, input.seats, input.speaker);
        let current_mood = input.seats[input.speaker.index()]
            .mood
            .unwrap_or_else(|| input.persona_id.initial_mood());
        let block3 = self.seat_trajectories[input.speaker.index()].format_block(
            Some(input.view),
            input.facts,
            current_mood,
            input.hands_revealed,
        );
        let block4 = self.transcript.format_block(input.seats, input.speaker);

        let seated_humans: Vec<String> = input
            .seats
            .iter()
            .filter(|info| info.kind == SeatKind::Human)
            .map(|info| info.nickname.clone())
            .collect();
        let block5 = input
            .persona_memory
            .build_prompt_memory_context(input.persona_id, &seated_humans)
            .unwrap_or_else(|| {
                "【5. 跨对局牌友记忆】\n本桌暂无过往跨对局牌友记忆（首次同桌或暂无历史账本）。"
                    .to_string()
            });

        let block6 = format!(
            "【6. 现实时间语境】\n当前现实时间：{}（闲聊时可自然贴合当下生活时段，不必硬凑）。",
            input.local_time_zh
        );

        format!("{block1}\n\n{block2}\n\n{block3}\n\n{block4}\n\n{block5}\n\n{block6}")
    }
}

#[cfg(test)]
mod tests {
    use mahjong_ai::analyze_hand_facts;
    use mahjong_core::{
        DiscardEntry, HandOutcome, HandSettlement, MatchMode, MeldView, PlayerHandView, PlayerView,
        PublicPhase, PublicSeatView, RevealedHand, Ruleset, ScoreItem, Seat, Tile, TuidaohuRuleset,
        TurnView, WinKind, Wind, WinnerSettlement,
    };

    use super::{
        format_local_time_zh, format_public_table_portrait, MatchBanterContext, MatchChronicle,
        SixBlockInput, TableTranscript, TABLE_TRANSCRIPT_CAPACITY,
    };
    use crate::persona::{PersonaId, PersonaMood};
    use crate::persona_memory::{PersonaMemoryNoteTrigger, PersonaMemoryStore};
    use crate::protocol::{QuickEmote, SeatInfo, SeatKind};

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
                mood: Some(PersonaMood::Deadpan),
                voice_pack_id: None,
            },
        ]
    }

    fn dummy_view(viewer: Seat) -> PlayerView {
        PlayerView {
            viewer,
            mode: MatchMode::EastOnly,
            rule_profile: TuidaohuRuleset.profile(),
            phase: PublicPhase::Playing,
            hand: PlayerHandView {
                closed_tiles: vec![
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
                ],
                drawn_tile: None,
            },
            seats: Seat::ALL.map(|seat| PublicSeatView {
                seat,
                score: 0,
                hand_count: 13,
                river: Vec::new(),
                melds: Vec::new(),
                last_draw_or_meld_seq: 0,
            }),
            wall_remaining: 42,
            turn: TurnView {
                wind_round: Wind::East,
                hand_number: 2,
                dealer_streak: 1,
                dealer: Seat::East,
                active_seat: Some(viewer),
            },
            legal_actions: Vec::new(),
            last_settlement: None,
            ready_seats: None,
            match_summary: None,
            seq: 20,
        }
    }

    #[test]
    fn local_time_formats_weekdays_and_time_of_day_deterministically() {
        assert_eq!(format_local_time_zh(6, 23), "周六深夜（周末）");
        assert_eq!(format_local_time_zh(6, 1), "周六深夜（周末）");
        assert_eq!(format_local_time_zh(3, 12), "周三中午（工作日）");
        assert_eq!(format_local_time_zh(5, 20), "周五晚上（周五晚，快周末了）");
        assert_eq!(
            format_local_time_zh(0, 21),
            "周日晚上（周末收尾，明早工作日）"
        );
        assert_eq!(format_local_time_zh(1, 6), "周一清晨（工作日）");
        assert_eq!(format_local_time_zh(2, 17), "周二傍晚（工作日）");
    }

    #[test]
    fn chronicle_tracks_hand_history_rankings_and_win_loss_streaks() {
        let seats = sample_seats();
        let mut chronicle = MatchChronicle::default();

        let make_win = |hand_number: u8, dealer_streak: u32, deltas: [i32; 4], scores: [i32; 4]| {
            HandSettlement {
                wind_round: Wind::East,
                hand_number,
                dealer_streak,
                dealer: Seat::East,
                outcome: HandOutcome::Win {
                    winners: vec![WinnerSettlement {
                        seat: Seat::East,
                        win_kind: WinKind::Ron,
                        items: vec![ScoreItem {
                            code: "menqing".to_string(),
                            label: "门前清".to_string(),
                            fan: 1,
                        }],
                        total_fan: 1,
                        base_points: 2,
                    }],
                    discarder: Some(Seat::South),
                    winning_tile: Tile::new(0).unwrap(),
                },
                kong_transfers: Vec::new(),
                score_deltas: deltas,
                new_scores: scores,
                revealed_hands: Seat::ALL.map(|seat| RevealedHand {
                    seat,
                    closed_tiles: Vec::new(),
                    drawn_tile: None,
                    melds: Vec::new(),
                }),
            }
        };

        chronicle.record_hand_settled(&make_win(1, 0, [4, -4, 0, 0], [4, -4, 0, 0]));
        chronicle.record_hand_settled(&make_win(1, 1, [4, -4, 0, 0], [8, -8, 0, 0]));

        let mut view = dummy_view(Seat::South);
        view.seats[Seat::East.index()].score = 8;
        view.seats[Seat::South.index()].score = -8;
        let text = chronicle.format_block(&view, &seats, Seat::South);

        assert!(text.contains("东风战 东二局"));
        assert!(text.contains("连庄 1 次"));
        assert!(text.contains("第1名 上家 小王 +8分"));
        assert!(text.contains("第4名 自家 大川 -8分"));
        assert!(text.contains("上家 小王 连胡 2 小局"));
        assert!(text.contains("自家 大川 连失分 2 小局"));
        assert!(text.contains("门前清"));
        assert!(!text.contains("小王（上家）"));
        assert!(!text.contains("大川（自家）"));
    }

    #[test]
    fn public_portrait_captures_cadence_and_suits_while_masking_opponent_concealed_kong() {
        let seats = sample_seats();
        let mut view = dummy_view(Seat::South);
        // East (opponent) has a ConcealedKong (even defensively test if tile were Some!)
        // plus 4 discards heavily concentrated in 万 with consecutive 摸切.
        view.seats[Seat::East.index()]
            .melds
            .push(MeldView::ConcealedKong {
                tile: Some(Tile::new(32).unwrap()), // 发财 — must STILL be masked because East != South!
                seq: 5,
            });
        view.seats[Seat::East.index()].river = vec![
            DiscardEntry {
                tile: Tile::new(0).unwrap(),
                is_tsumogiri: false,
                claimed_by: None,
                seq: 1,
            },
            DiscardEntry {
                tile: Tile::new(1).unwrap(),
                is_tsumogiri: true,
                claimed_by: None,
                seq: 2,
            },
            DiscardEntry {
                tile: Tile::new(2).unwrap(),
                is_tsumogiri: true,
                claimed_by: None,
                seq: 3,
            },
            DiscardEntry {
                tile: Tile::new(3).unwrap(),
                is_tsumogiri: true,
                claimed_by: None,
                seq: 4,
            },
        ];
        // South (speaker) has own ConcealedKong of 红中 — own concealed kong MAY be shown!
        view.seats[Seat::South.index()]
            .melds
            .push(MeldView::ConcealedKong {
                tile: Some(Tile::new(31).unwrap()), // 红中
                seq: 6,
            });

        let portrait = format_public_table_portrait(&view, &seats, Seat::South);
        assert!(portrait.contains("牌墙进度：剩余 42 张"));
        assert!(portrait.contains("自家 大川：暗杠红中"));
        assert!(portrait.contains("上家 小王：暗杠一手（牌面保密未知）"));
        assert!(
            !portrait.contains("发财"),
            "opponent concealed kong leaked: {portrait}"
        );
        assert!(portrait.contains("连摸切3手"));
        assert!(portrait.contains("集中舍万"));
    }

    #[test]
    fn table_transcript_enforces_rolling_cap_and_tracks_turns_since_last_spoke() {
        let seats = sample_seats();
        let mut transcript = TableTranscript::default();

        // Initially South has never spoken.
        let before = transcript.format_block(&seats, Seat::South);
        assert!(before.contains("本场对局还没开过口"));

        transcript.record_banter(
            Seat::South,
            "大川".to_string(),
            PersonaMood::Smug,
            "第一句台词".to_string(),
            "东一局·余60张".to_string(),
        );
        assert_eq!(transcript.turns_since_last_spoke(Seat::South), Some(0));

        // Advance 3 turns for South (and 12 discards total = 3 rounds).
        for _ in 0..3 {
            for s in Seat::ALL {
                transcript.record_discard(s);
            }
        }
        assert_eq!(transcript.turns_since_last_spoke(Seat::South), Some(3));

        // Push 22 more quick emotes -> rolling window caps at 20, evicting oldest entries.
        for idx in 1..=22u32 {
            transcript.record_quick_emote(
                Seat::East,
                "小王".to_string(),
                QuickEmote::GoingBig,
                format!("快捷第{idx}句"),
                "东二局·余40张".to_string(),
            );
        }
        assert_eq!(transcript.entries().len(), TABLE_TRANSCRIPT_CAPACITY);

        let formatted = transcript.format_block(&seats, Seat::South);
        assert!(formatted.contains("你距离上次开口已过 3 巡"));
        assert!(!formatted.contains("第一句台词"));
        assert!(!formatted.contains("快捷第2句"));
        assert!(formatted.contains("快捷第3句"));
        assert!(formatted.contains("快捷第22句"));
        assert!(formatted.contains("上家 小王 发了快捷互动"));
    }

    #[test]
    fn six_block_formatter_assembles_all_six_blocks_with_scoped_memory() {
        let seats = sample_seats();
        let view = dummy_view(Seat::South);
        let facts = analyze_hand_facts(&view, None);
        let mut ctx = MatchBanterContext::default();
        ctx.seat_trajectories[Seat::South.index()].reset_for_hand(facts.clone());
        ctx.record_ai_hand_step(Seat::South, facts.clone(), Some(Tile::new(27).unwrap()));

        let mut memory = PersonaMemoryStore::in_memory();
        memory.record_grudge_note(
            PersonaId::QuickHothead,
            "小王",
            "小王上次抢杠胡欠账",
            PersonaMemoryNoteTrigger::RobKongHu,
            1000,
        );
        memory.record_grudge_note(
            PersonaId::QuickHothead,
            "未入座路人",
            "秘密账本绝不能漏出",
            PersonaMemoryNoteTrigger::MatchFinished,
            2000,
        );

        let text = ctx.format_six_blocks(&SixBlockInput {
            speaker: Seat::South,
            persona_id: PersonaId::QuickHothead,
            seats: &seats,
            view: &view,
            facts: Some(&facts),
            hands_revealed: false,
            persona_memory: &memory,
            local_time_zh: "周六深夜（周末）",
        });

        for heading in [
            "【1. 本场对局战况编年史】",
            "【2. 本小局公开动态与牌河画像】",
            "【3. 自家手牌、预计算手牌事实与牌运轨迹】",
            "【4. 跨小局牌桌对话流",
            "牌友记忆",
            "【6. 现实时间语境】",
        ] {
            assert!(text.contains(heading), "missing {heading} in:\n{text}");
        }
        assert!(text.contains("周六深夜（周末）"));
        assert!(text.contains("小王上次抢杠胡欠账"));
        assert!(!text.contains("未入座路人"));
        assert!(!text.contains("秘密账本绝不能漏出"));
    }
}
