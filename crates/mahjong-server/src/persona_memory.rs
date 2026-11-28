//! Cross-match persistent persona memory (`牌友记忆`): exact per-player ledgers and
//! rolling 8-entry grudge notes ("恩怨小本本") stored atomically in `<data_dir>/persona_memory.json`.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use mahjong_core::{HandOutcome, HandSettlement, MatchSummary, RevealedMeld, Seat, WinKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::banter::{format_tile_zh, persona_voice_guide};
use crate::llm::MemoryNoteRequest;
use crate::persona::PersonaId;
use crate::protocol::{SeatInfo, SeatKind};
use crate::session::SeatRecord;
use crate::storage::{atomic_write_bytes, normalize_nickname, normalize_single_line_text};

/// Current schema version for `persona_memory.json` and HTTP responses.
pub const PERSONA_MEMORY_SCHEMA_VERSION: u32 = 1;

/// Maximum rolling FIFO grudge notes retained per `(PersonaId, normalized_player_nickname)` pair.
pub const MAX_GRUDGE_NOTES: usize = 8;

/// Maximum Unicode character length enforced for each grudge note (`<= 30` chars).
pub const MAX_GRUDGE_NOTE_CHARS: usize = 30;

/// Minimum winning fan count considered a memorable highlight hand for grudge-note distillation.
pub const MEMORABLE_HIGH_FAN_THRESHOLD: u32 = 4;

const PERSONA_MEMORY_FILE: &str = "persona_memory.json";

/// Trigger category that caused a persona grudge note to be recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PersonaMemoryNoteTrigger {
    /// High-fan winning hand (`>= 4` fan).
    HighFan,
    /// Robbing an added kong (`抢杠胡`).
    RobKongHu,
    /// Multi-ron (`一炮多响`).
    MultiRon,
    /// End-of-match summary (`对局`结束).
    MatchFinished,
}

/// Deterministic head-to-head ledger updated in Rust after every settled hand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonaPlayerLedger {
    /// Total settled hands where this persona and this human were seated together.
    pub hands_played: u32,
    /// Settled hands won by this persona while seated with this human.
    pub persona_wins_against_player: u32,
    /// Settled hands won by this human while seated with this persona.
    pub player_wins_against_persona: u32,
    /// Times this persona dealt into (or had a kong robbed by) this human.
    pub persona_dealt_into_player: u32,
    /// Times this human dealt into (or had a kong robbed by) this persona.
    pub player_dealt_into_persona: u32,
    /// Highest single-hand fan count seen in wins involving either this persona or this human.
    pub highest_fan_seen: u32,
    /// Cumulative score delta for this persona across hands played with this human.
    #[ts(type = "number")]
    pub net_score_delta: i64,
    /// Total `碰` melds declared by this human across hands played with this persona.
    pub player_pon_count: u32,
    /// Total `明杠`/`补杠`/`暗杠` melds declared by this human across hands played with this persona.
    pub player_kong_count: u32,
}

/// One concise persona-voiced grudge note ("恩怨小本本") entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonaGrudgeNote {
    /// Concise persona-voiced Chinese note (`<= 30` chars).
    pub summary: String,
    /// Unix epoch timestamp in milliseconds when recorded.
    #[ts(type = "number")]
    pub created_at_ms: u64,
    /// Event category that triggered this note.
    pub trigger: PersonaMemoryNoteTrigger,
}

/// Combined ledger and rolling grudge notes for one `(PersonaId, normalized_player_nickname)` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonaPlayerMemory {
    /// Normalized human player nickname.
    pub player_nickname: String,
    /// Exact deterministic ledger.
    pub ledger: PersonaPlayerLedger,
    /// Rolling FIFO grudge notes (at most 8, oldest first / newest last).
    pub grudge_notes: Vec<PersonaGrudgeNote>,
    /// Unix epoch timestamp in milliseconds of the last update.
    #[ts(type = "number")]
    pub updated_at_ms: u64,
}

/// Grouped memory entries for one built-in `PersonaId` returned by `GET /api/persona-memory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonaMemoryEntry {
    /// Canonical persona identifier.
    pub persona_id: PersonaId,
    /// Player memory records for this persona, ordered by most recent update descending.
    pub players: Vec<PersonaPlayerMemory>,
}

/// Response payload for `GET /api/persona-memory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonaMemoryResponse {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Total number of non-empty `(persona_id, player_nickname)` memory pairs.
    pub total_pairs: u32,
    /// Total number of stored grudge notes across all pairs.
    pub total_notes: u32,
    /// Memory entries for all five built-in personas in canonical display order.
    pub personas: Vec<PersonaMemoryEntry>,
}

/// Request payload for `POST /api/persona-memory/clear`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClearPersonaMemoryRequest {
    /// Must be explicitly `true` to confirm clearing memory on disk and in memory.
    pub confirm: bool,
}

/// Response payload for `POST /api/persona-memory/clear`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClearPersonaMemoryResponse {
    /// `true` once in-memory and on-disk persona memory have been cleared.
    pub cleared: bool,
    /// Number of `(persona_id, player_nickname)` records removed.
    pub cleared_pairs: u32,
    /// Number of grudge notes removed.
    pub cleared_notes: u32,
}

/// Asynchronous grudge-note generation job scheduled via `ai_pool::spawn_scheduled_work`.
#[derive(Debug, Clone)]
pub(crate) struct MemoryNoteJob {
    pub persona_id: PersonaId,
    pub player_nickname: String,
    pub trigger: PersonaMemoryNoteTrigger,
    pub request: MemoryNoteRequest,
}

fn default_schema_version() -> u32 {
    PERSONA_MEMORY_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPersonaPair {
    persona_id: PersonaId,
    player_nickname: String,
    #[serde(default)]
    ledger: PersonaPlayerLedger,
    #[serde(default)]
    grudge_notes: Vec<PersonaGrudgeNote>,
    #[serde(default)]
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPersonaMemory {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    records: Vec<PersistedPersonaPair>,
}

impl Default for PersistedPersonaMemory {
    fn default() -> Self {
        Self {
            schema_version: PERSONA_MEMORY_SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

/// Normalizes whitespace and clamps a grudge note to `<= 30` Unicode characters.
#[must_use]
pub fn normalize_grudge_note(raw: &str) -> String {
    normalize_single_line_text(raw)
        .chars()
        .take(MAX_GRUDGE_NOTE_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

struct SeatedParticipants {
    ai: Vec<(Seat, PersonaId)>,
    humans: Vec<(Seat, String)>,
}

fn extract_ai_and_human_seats<'a>(
    seats: impl IntoIterator<Item = &'a SeatInfo>,
) -> SeatedParticipants {
    let mut ai = Vec::new();
    let mut humans = Vec::new();
    for info in seats {
        match info.kind {
            SeatKind::Ai => {
                if let Some(pid) = info.persona_id {
                    ai.push((info.seat, pid));
                }
            }
            SeatKind::Human => {
                let norm = normalize_nickname(&info.nickname);
                if !norm.is_empty() {
                    humans.push((info.seat, norm));
                }
            }
            SeatKind::Empty => {}
        }
    }
    SeatedParticipants { ai, humans }
}

/// Persistent cross-match store for `PersonaMemory` (`牌友记忆`).
#[derive(Debug, Clone)]
pub struct PersonaMemoryStore {
    data_dir: Option<PathBuf>,
    pairs: BTreeMap<(PersonaId, String), PersonaPlayerMemory>,
}

impl PersonaMemoryStore {
    /// Creates an ephemeral in-memory store (used when `data_dir` is unset).
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            data_dir: None,
            pairs: BTreeMap::new(),
        }
    }

    /// Opens `<data_dir>/persona_memory.json`, tolerating missing or corrupt files without failing startup.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if `data_dir` cannot be created or if an existing file cannot be read
    /// for reasons other than `NotFound`.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let path = dir.join(PERSONA_MEMORY_FILE);
        let pairs = load_pairs_tolerant(&path)?;
        Ok(Self {
            data_dir: Some(dir),
            pairs,
        })
    }

    /// Deterministically updates the exact ledger for every `(AI persona, human player)` pair
    /// seated at hand settlement, then persists atomically if backed by disk.
    pub fn record_hand_settled(
        &mut self,
        seats: &[SeatInfo; 4],
        settlement: &HandSettlement,
        timestamp_ms: u64,
    ) {
        let SeatedParticipants {
            ai: ai_seats,
            humans: human_seats,
        } = extract_ai_and_human_seats(seats.iter());

        if ai_seats.is_empty() || human_seats.is_empty() {
            return;
        }

        let mut changed = false;
        for &(ai_seat, persona_id) in &ai_seats {
            let persona_delta = i64::from(settlement.score_deltas[ai_seat.index()]);
            for (human_seat, human_nick) in &human_seats {
                let entry = self
                    .pairs
                    .entry((persona_id, human_nick.clone()))
                    .or_insert_with(|| PersonaPlayerMemory {
                        player_nickname: human_nick.clone(),
                        ledger: PersonaPlayerLedger::default(),
                        grudge_notes: Vec::new(),
                        updated_at_ms: timestamp_ms,
                    });

                entry.ledger.hands_played = entry.ledger.hands_played.saturating_add(1);
                entry.ledger.net_score_delta =
                    entry.ledger.net_score_delta.saturating_add(persona_delta);

                if let HandOutcome::Win {
                    winners, discarder, ..
                } = &settlement.outcome
                {
                    let mut max_fan = 0u32;
                    for winner in winners {
                        if winner.seat == ai_seat {
                            entry.ledger.persona_wins_against_player =
                                entry.ledger.persona_wins_against_player.saturating_add(1);
                            if *discarder == Some(*human_seat) {
                                entry.ledger.player_dealt_into_persona =
                                    entry.ledger.player_dealt_into_persona.saturating_add(1);
                            }
                            max_fan = max_fan.max(u32::from(winner.total_fan));
                        } else if winner.seat == *human_seat {
                            entry.ledger.player_wins_against_persona =
                                entry.ledger.player_wins_against_persona.saturating_add(1);
                            if *discarder == Some(ai_seat) {
                                entry.ledger.persona_dealt_into_player =
                                    entry.ledger.persona_dealt_into_player.saturating_add(1);
                            }
                            max_fan = max_fan.max(u32::from(winner.total_fan));
                        }
                    }
                    if max_fan > entry.ledger.highest_fan_seen {
                        entry.ledger.highest_fan_seen = max_fan;
                    }
                }

                let (pon_count, kong_count) =
                    count_melds(&settlement.revealed_hands[human_seat.index()].melds);
                entry.ledger.player_pon_count =
                    entry.ledger.player_pon_count.saturating_add(pon_count);
                entry.ledger.player_kong_count =
                    entry.ledger.player_kong_count.saturating_add(kong_count);
                entry.updated_at_ms = timestamp_ms;
                changed = true;
            }
        }

        if changed {
            self.save_best_effort();
        }
    }

    /// Appends one persona-voiced grudge note, enforcing the 30-char clamp and FIFO cap of 8 notes,
    /// then persists atomically if backed by disk.
    pub fn record_grudge_note(
        &mut self,
        persona_id: PersonaId,
        raw_nickname: &str,
        raw_summary: &str,
        trigger: PersonaMemoryNoteTrigger,
        created_at_ms: u64,
    ) -> bool {
        let nickname = normalize_nickname(raw_nickname);
        if nickname.is_empty() {
            return false;
        }
        let summary = normalize_grudge_note(raw_summary);
        if summary.is_empty() {
            return false;
        }

        let entry = self
            .pairs
            .entry((persona_id, nickname.clone()))
            .or_insert_with(|| PersonaPlayerMemory {
                player_nickname: nickname,
                ledger: PersonaPlayerLedger::default(),
                grudge_notes: Vec::new(),
                updated_at_ms: created_at_ms,
            });

        entry.grudge_notes.push(PersonaGrudgeNote {
            summary,
            created_at_ms,
            trigger,
        });
        if entry.grudge_notes.len() > MAX_GRUDGE_NOTES {
            let excess = entry.grudge_notes.len() - MAX_GRUDGE_NOTES;
            entry.grudge_notes.drain(0..excess);
        }
        entry.updated_at_ms = created_at_ms;
        self.save_best_effort();
        true
    }

    /// Returns a read-only lookup for a single `(PersonaId, raw_nickname)` pair.
    #[must_use]
    pub fn get(&self, persona_id: PersonaId, raw_nickname: &str) -> Option<&PersonaPlayerMemory> {
        let norm = normalize_nickname(raw_nickname);
        if norm.is_empty() {
            return None;
        }
        self.pairs.get(&(persona_id, norm))
    }

    /// Builds a concise prompt context block scoped strictly to the currently seated human nicknames.
    ///
    /// Never includes records belonging to players who are not in `seated_human_nicknames`.
    #[must_use]
    pub fn build_prompt_memory_context(
        &self,
        persona_id: PersonaId,
        seated_human_nicknames: &[String],
    ) -> Option<String> {
        let mut seen = HashSet::new();
        let mut lines = Vec::new();

        for raw in seated_human_nicknames {
            let norm = normalize_nickname(raw);
            if norm.is_empty() || !seen.insert(norm.clone()) {
                continue;
            }
            let Some(record) = self.pairs.get(&(persona_id, norm)) else {
                continue;
            };
            if record.ledger.hands_played == 0 && record.grudge_notes.is_empty() {
                continue;
            }
            let l = &record.ledger;
            let mut block = format!(
                "- 对「{}」：共交手 {} 小局，你胡 {} 次 / 对方胡 {} 次，你点炮给对方 {} 次 / 对方点炮给你 {} 次，最高番 {} 番，你同桌累计盈亏 {:+} 分，对方累计碰 {} 次、杠 {} 次。",
                record.player_nickname,
                l.hands_played,
                l.persona_wins_against_player,
                l.player_wins_against_persona,
                l.persona_dealt_into_player,
                l.player_dealt_into_persona,
                l.highest_fan_seen,
                l.net_score_delta,
                l.player_pon_count,
                l.player_kong_count,
            );
            if !record.grudge_notes.is_empty() {
                let notes_joined = record
                    .grudge_notes
                    .iter()
                    .rev()
                    .enumerate()
                    .map(|(idx, n)| format!("{}) {}", idx + 1, n.summary))
                    .collect::<Vec<_>>()
                    .join("；");
                let _ = write!(block, "\n  恩怨小本本：{notes_joined}");
            }
            lines.push(block);
        }

        if lines.is_empty() {
            return None;
        }

        Some(format!(
            "【5. 跨对局牌友记忆（仅含当前同桌熟人，聊天时顺手化用旧账即可，别生硬念统计数字）】\n{}",
            lines.join("\n")
        ))
    }

    /// Produces the structured snapshot served by `GET /api/persona-memory`.
    #[must_use]
    pub fn snapshot(&self) -> PersonaMemoryResponse {
        let mut total_pairs = 0u32;
        let mut total_notes = 0u32;

        let personas = PersonaId::ALL
            .into_iter()
            .map(|persona_id| {
                let mut players: Vec<PersonaPlayerMemory> = self
                    .pairs
                    .iter()
                    .filter(|((pid, _), _)| *pid == persona_id)
                    .map(|(_, mem)| mem.clone())
                    .collect();
                players.sort_by(|a, b| {
                    b.updated_at_ms
                        .cmp(&a.updated_at_ms)
                        .then_with(|| b.ledger.hands_played.cmp(&a.ledger.hands_played))
                        .then_with(|| a.player_nickname.cmp(&b.player_nickname))
                });
                total_pairs =
                    total_pairs.saturating_add(u32::try_from(players.len()).unwrap_or(u32::MAX));
                for p in &players {
                    total_notes = total_notes
                        .saturating_add(u32::try_from(p.grudge_notes.len()).unwrap_or(u32::MAX));
                }
                PersonaMemoryEntry {
                    persona_id,
                    players,
                }
            })
            .collect();

        PersonaMemoryResponse {
            schema_version: PERSONA_MEMORY_SCHEMA_VERSION,
            total_pairs,
            total_notes,
            personas,
        }
    }

    /// Clears all in-memory and persisted persona memories atomically.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if clearing `persona_memory.json` on disk fails.
    pub fn clear(&mut self) -> Result<ClearPersonaMemoryResponse, std::io::Error> {
        let cleared_pairs = u32::try_from(self.pairs.len()).unwrap_or(u32::MAX);
        let cleared_notes = self
            .pairs
            .values()
            .map(|m| u32::try_from(m.grudge_notes.len()).unwrap_or(u32::MAX))
            .fold(0u32, u32::saturating_add);

        if let Some(dir) = &self.data_dir {
            let empty = PersistedPersonaMemory::default();
            let mut bytes = serde_json::to_vec_pretty(&empty).map_err(std::io::Error::other)?;
            bytes.push(b'\n');
            atomic_write_bytes(&dir.join(PERSONA_MEMORY_FILE), &bytes)?;
        }

        self.pairs.clear();
        Ok(ClearPersonaMemoryResponse {
            cleared: true,
            cleared_pairs,
            cleared_notes,
        })
    }

    fn save_best_effort(&self) {
        let Some(dir) = &self.data_dir else {
            return;
        };
        let path = dir.join(PERSONA_MEMORY_FILE);
        let records = self
            .pairs
            .iter()
            .map(|((persona_id, _), mem)| PersistedPersonaPair {
                persona_id: *persona_id,
                player_nickname: mem.player_nickname.clone(),
                ledger: mem.ledger.clone(),
                grudge_notes: mem.grudge_notes.clone(),
                updated_at_ms: mem.updated_at_ms,
            })
            .collect();
        let persisted = PersistedPersonaMemory {
            schema_version: PERSONA_MEMORY_SCHEMA_VERSION,
            records,
        };
        match serde_json::to_vec_pretty(&persisted) {
            Ok(mut bytes) => {
                bytes.push(b'\n');
                if let Err(err) = atomic_write_bytes(&path, &bytes) {
                    eprintln!("warning: failed to persist {}: {err}", path.display());
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to serialize persona memory for {}: {err}",
                    path.display()
                );
            }
        }
    }
}

fn count_melds(melds: &[RevealedMeld]) -> (u32, u32) {
    let mut pons = 0u32;
    let mut kongs = 0u32;
    for meld in melds {
        match meld {
            RevealedMeld::Pon { .. } => pons += 1,
            RevealedMeld::ExposedKong { .. }
            | RevealedMeld::AddedKong { .. }
            | RevealedMeld::ConcealedKong { .. } => kongs += 1,
            RevealedMeld::Chi { .. } => {}
        }
    }
    (pons, kongs)
}

fn load_pairs_tolerant(
    path: &Path,
) -> Result<BTreeMap<(PersonaId, String), PersonaPlayerMemory>, std::io::Error> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => return Err(err),
    };

    let parsed: PersistedPersonaMemory = match serde_json::from_str(&contents) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "warning: failed to parse {}: {err}; backing up and starting with empty persona memory",
                path.display()
            );
            let backup = path.with_extension("json.corrupt");
            let _ = fs::rename(path, backup);
            return Ok(BTreeMap::new());
        }
    };

    let mut pairs = BTreeMap::new();
    for pair in parsed.records {
        let norm = normalize_nickname(&pair.player_nickname);
        if norm.is_empty() {
            continue;
        }
        let mut notes: Vec<PersonaGrudgeNote> = pair
            .grudge_notes
            .into_iter()
            .filter_map(|mut note| {
                note.summary = normalize_grudge_note(&note.summary);
                (!note.summary.is_empty()).then_some(note)
            })
            .collect();
        if notes.len() > MAX_GRUDGE_NOTES {
            let excess = notes.len() - MAX_GRUDGE_NOTES;
            notes.drain(0..excess);
        }
        pairs.insert(
            (pair.persona_id, norm.clone()),
            PersonaPlayerMemory {
                player_nickname: norm,
                ledger: pair.ledger,
                grudge_notes: notes,
                updated_at_ms: pair.updated_at_ms,
            },
        );
    }
    Ok(pairs)
}

fn note_system_prompt(persona_id: PersonaId) -> String {
    let voice = persona_voice_guide(persona_id);
    format!(
        "{voice}\n\
请把刚发生的牌局名场面或终局结果浓缩成 1 句写进「恩怨小本本」的熟人记仇或得意笔记。\n\
严格要求：\n\
1. 仅输出合法 JSON：{{\"note\":\"...\"}}，严禁输出思考过程、<think> 标签或多余解释。\n\
2. note 必须是 8 到 28 字的鲜活中文口语单句（绝不超过 30 字），贴合你的真实性格口吻并点明对方昵称与核心事件。\n\
3. 只引用输入中明确给出的事实，严禁编造牌名或数字。"
    )
}

/// Plans asynchronous grudge-note jobs for memorable settled hands (`>= 4` fan, `抢杠胡`, `一炮多响`).
pub(crate) fn plan_memorable_hand_note_jobs(
    seats: &[SeatRecord; 4],
    settlement: &HandSettlement,
) -> Vec<MemoryNoteJob> {
    let HandOutcome::Win {
        winners,
        discarder,
        winning_tile,
    } = &settlement.outcome
    else {
        return Vec::new();
    };

    let has_rob_kong = winners.iter().any(|w| w.win_kind == WinKind::RobKongHu);
    let is_multi_ron = winners.len() >= 2;
    let max_fan = winners
        .iter()
        .map(|w| u32::from(w.total_fan))
        .max()
        .unwrap_or(0);
    let is_high_fan = max_fan >= MEMORABLE_HIGH_FAN_THRESHOLD;

    let trigger = if has_rob_kong {
        PersonaMemoryNoteTrigger::RobKongHu
    } else if is_multi_ron {
        PersonaMemoryNoteTrigger::MultiRon
    } else if is_high_fan {
        PersonaMemoryNoteTrigger::HighFan
    } else {
        return Vec::new();
    };

    let tile_zh = format_tile_zh(*winning_tile);
    let SeatedParticipants {
        ai: ai_seats,
        humans: human_seats,
    } = extract_ai_and_human_seats(seats.iter().map(|s| &s.info));

    let mut jobs = Vec::new();
    for &(ai_seat, persona_id) in &ai_seats {
        for (human_seat, human_nick) in &human_seats {
            let involves_pair = winners
                .iter()
                .any(|w| w.seat == ai_seat || w.seat == *human_seat)
                || *discarder == Some(ai_seat)
                || *discarder == Some(*human_seat);
            if !involves_pair {
                continue;
            }

            let summary = winners
                .iter()
                .map(|w| {
                    let who = normalize_nickname(&seats[w.seat.index()].info.nickname);
                    let kind = match w.win_kind {
                        WinKind::Tsumo => "自摸",
                        WinKind::Ron => "点炮胡",
                        WinKind::RobKongHu => "抢杠胡",
                    };
                    format!("{who}{kind}{tile_zh}（{}番）", w.total_fan)
                })
                .collect::<Vec<_>>()
                .join("；");
            let discarder_text = discarder
                .map(|d| {
                    format!(
                        "，点炮/被抢杠胡者：{}",
                        seats[d.index()].info.nickname.trim()
                    )
                })
                .unwrap_or_default();
            let user_prompt = format!(
                "对象玩家：「{human_nick}」\n\
本小局名场面：{summary}{discarder_text}。\n\
本小局得分变化：你 {:+} 分，「{human_nick}」 {:+} 分。\n\
请以你的角色口吻写 1 句不超过 30 字的恩怨小本本笔记记住「{human_nick}」。",
                settlement.score_deltas[ai_seat.index()],
                settlement.score_deltas[human_seat.index()],
            );
            jobs.push(MemoryNoteJob {
                persona_id,
                player_nickname: human_nick.clone(),
                trigger,
                request: MemoryNoteRequest::new(note_system_prompt(persona_id), user_prompt),
            });
        }
    }
    jobs
}

/// Plans asynchronous grudge-note jobs when a match finishes cleanly (`hands_played > 0`).
pub(crate) fn plan_match_finished_note_jobs(
    seats: &[SeatRecord; 4],
    summary: &MatchSummary,
) -> Vec<MemoryNoteJob> {
    if summary.hands_played == 0 {
        return Vec::new();
    }

    let SeatedParticipants {
        ai: ai_seats,
        humans: human_seats,
    } = extract_ai_and_human_seats(seats.iter().map(|s| &s.info));

    let mut jobs = Vec::new();
    for &(ai_seat, persona_id) in &ai_seats {
        for (human_seat, human_nick) in &human_seats {
            let user_prompt = format!(
                "对象玩家：「{human_nick}」\n\
本场对局共打了 {} 小局已结束。\n\
终局净盈亏：你 {:+} 分（终局 {} 分），「{human_nick}」 {:+} 分（终局 {} 分）。\n\
请以你的角色口吻写 1 句不超过 30 字的恩怨小本本总结，方便下次开桌翻旧账。",
                summary.hands_played,
                summary.net_deltas[ai_seat.index()],
                summary.final_scores[ai_seat.index()],
                summary.net_deltas[human_seat.index()],
                summary.final_scores[human_seat.index()],
            );
            jobs.push(MemoryNoteJob {
                persona_id,
                player_nickname: human_nick.clone(),
                trigger: PersonaMemoryNoteTrigger::MatchFinished,
                request: MemoryNoteRequest::new(note_system_prompt(persona_id), user_prompt),
            });
        }
    }
    jobs
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use mahjong_core::{
        HandOutcome, HandSettlement, RevealedHand, RevealedMeld, ScoreItem, Seat, Tile, WinKind,
        Wind, WinnerSettlement,
    };

    use super::{
        normalize_grudge_note, PersonaMemoryNoteTrigger, PersonaMemoryStore, MAX_GRUDGE_NOTES,
        MAX_GRUDGE_NOTE_CHARS, PERSONA_MEMORY_SCHEMA_VERSION,
    };
    use crate::persona::PersonaId;
    use crate::protocol::{SeatInfo, SeatKind};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{prefix}_{}_{seq}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample_seats(east_nick: &str) -> [SeatInfo; 4] {
        [
            SeatInfo {
                seat: Seat::East,
                kind: SeatKind::Human,
                nickname: east_nick.to_string(),
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
                nickname: PersonaId::TeaSarcasm.display_name().to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: Some(PersonaId::TeaSarcasm),
                mood: None,
                voice_pack_id: None,
            },
            SeatInfo {
                seat: Seat::West,
                kind: SeatKind::Ai,
                nickname: PersonaId::QuickHothead.display_name().to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: Some(PersonaId::QuickHothead),
                mood: None,
                voice_pack_id: None,
            },
            SeatInfo {
                seat: Seat::North,
                kind: SeatKind::Ai,
                nickname: PersonaId::TiltedMaster.display_name().to_string(),
                connected: true,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: Some(PersonaId::TiltedMaster),
                mood: None,
                voice_pack_id: None,
            },
        ]
    }

    fn sample_settlement(
        winner: Seat,
        discarder: Option<Seat>,
        win_kind: WinKind,
        total_fan: u8,
        score_deltas: [i32; 4],
        east_melds: Vec<RevealedMeld>,
    ) -> HandSettlement {
        HandSettlement {
            wind_round: Wind::East,
            hand_number: 1,
            dealer_streak: 0,
            dealer: Seat::East,
            outcome: HandOutcome::Win {
                winners: vec![WinnerSettlement {
                    seat: winner,
                    win_kind,
                    items: vec![ScoreItem {
                        code: "fan".to_string(),
                        label: "测试番".to_string(),
                        fan: total_fan,
                    }],
                    total_fan,
                    base_points: 2,
                }],
                discarder,
                winning_tile: Tile::new(0).unwrap(),
            },
            kong_transfers: Vec::new(),
            score_deltas,
            new_scores: score_deltas,
            revealed_hands: Seat::ALL.map(|seat| RevealedHand {
                seat,
                closed_tiles: Vec::new(),
                drawn_tile: None,
                melds: if seat == Seat::East {
                    east_melds.clone()
                } else {
                    Vec::new()
                },
            }),
        }
    }

    #[test]
    fn ledger_updates_persist_across_reopen_and_recover_from_corrupt_file() {
        let dir = TempDir::new("majjj_persona_memory_unit");
        let mut store = PersonaMemoryStore::open(&dir.0).expect("open store");
        let seats = sample_seats("  小王  ");

        // Hand 1: South (TeaSarcasm) deals into East (小王) for 4 fan; East had 1 Pon + 1 Kong.
        let s1 = sample_settlement(
            Seat::East,
            Some(Seat::South),
            WinKind::Ron,
            4,
            [16, -16, 0, 0],
            vec![
                RevealedMeld::Pon {
                    tile: Tile::new(0).unwrap(),
                    from_seat: Seat::South,
                    seq: 1,
                },
                RevealedMeld::ConcealedKong {
                    tile: Tile::new(31).unwrap(),
                    seq: 2,
                },
            ],
        );
        store.record_hand_settled(&seats, &s1, 1000);

        // Hand 2: East deals into South (TeaSarcasm) for 2 fan.
        let s2 = sample_settlement(
            Seat::South,
            Some(Seat::East),
            WinKind::Ron,
            2,
            [-4, 4, 0, 0],
            vec![RevealedMeld::Pon {
                tile: Tile::new(1).unwrap(),
                from_seat: Seat::West,
                seq: 3,
            }],
        );
        store.record_hand_settled(&seats, &s2, 2000);

        // Reopen from disk and verify exact ledger.
        let reopened = PersonaMemoryStore::open(&dir.0).expect("reopen store");
        let tea = reopened
            .get(PersonaId::TeaSarcasm, "小王")
            .expect("TeaSarcasm memory for 小王");
        assert_eq!(tea.ledger.hands_played, 2);
        assert_eq!(tea.ledger.persona_wins_against_player, 1);
        assert_eq!(tea.ledger.player_wins_against_persona, 1);
        assert_eq!(tea.ledger.persona_dealt_into_player, 1);
        assert_eq!(tea.ledger.player_dealt_into_persona, 1);
        assert_eq!(tea.ledger.highest_fan_seen, 4);
        assert_eq!(tea.ledger.net_score_delta, -12);
        assert_eq!(tea.ledger.player_pon_count, 2);
        assert_eq!(tea.ledger.player_kong_count, 1);

        // Corrupt JSON recovers gracefully without failing open.
        fs::write(dir.0.join("persona_memory.json"), "{corrupt").expect("write corrupt file");
        let recovered = PersonaMemoryStore::open(&dir.0).expect("corrupt file must not fail open");
        let snap = recovered.snapshot();
        assert_eq!(snap.schema_version, PERSONA_MEMORY_SCHEMA_VERSION);
        assert_eq!(snap.total_pairs, 0);
        assert_eq!(snap.total_notes, 0);
        assert!(dir.0.join("persona_memory.json.corrupt").exists());
    }

    #[test]
    fn grudge_notes_enforce_fifo_cap_of_eight_and_thirty_char_limit() {
        let mut store = PersonaMemoryStore::in_memory();
        let long_note = "小王这人手气太离谱了居然连续四次清一色自摸把老子血压直接拉满到外太空";
        assert!(long_note.chars().count() > MAX_GRUDGE_NOTE_CHARS);
        let normalized = normalize_grudge_note(long_note);
        assert_eq!(normalized.chars().count(), MAX_GRUDGE_NOTE_CHARS);

        for i in 1..=11u64 {
            let ok = store.record_grudge_note(
                PersonaId::TeaSarcasm,
                "小王",
                &format!("恩怨第{i}条：小王又来偷鸡"),
                PersonaMemoryNoteTrigger::HighFan,
                i * 1000,
            );
            assert!(ok);
        }

        let mem = store
            .get(PersonaId::TeaSarcasm, "小王")
            .expect("memory exists");
        assert_eq!(mem.grudge_notes.len(), MAX_GRUDGE_NOTES);
        assert_eq!(mem.grudge_notes[0].summary, "恩怨第4条：小王又来偷鸡");
        assert_eq!(mem.grudge_notes[7].summary, "恩怨第11条：小王又来偷鸡");
    }

    #[test]
    fn prompt_memory_context_strictly_scopes_to_seated_humans_without_leakage() {
        let mut store = PersonaMemoryStore::in_memory();
        store.record_grudge_note(
            PersonaId::TeaSarcasm,
            "小王",
            "小王上次抢杠胡欠我一笔债",
            PersonaMemoryNoteTrigger::RobKongHu,
            1000,
        );
        store.record_grudge_note(
            PersonaId::TeaSarcasm,
            "老李",
            "老李秘密账单绝对不能泄漏",
            PersonaMemoryNoteTrigger::MatchFinished,
            2000,
        );

        let ctx_wang = store
            .build_prompt_memory_context(PersonaId::TeaSarcasm, &["  小王 ".to_string()])
            .expect("memory context for 小王");
        assert!(ctx_wang.contains("小王上次抢杠胡欠我一笔债"));
        assert!(!ctx_wang.contains("老李"));
        assert!(!ctx_wang.contains("秘密账单"));

        let ctx_stranger =
            store.build_prompt_memory_context(PersonaId::TeaSarcasm, &["陌生人".to_string()]);
        assert!(ctx_stranger.is_none());
    }

    #[test]
    fn memory_note_system_prompts_use_humanized_personas_without_dealer_bias() {
        for pid in PersonaId::ALL {
            let prompt = super::note_system_prompt(pid);
            assert!(
                prompt.contains(pid.display_name()),
                "missing humanized name {} in note prompt",
                pid.display_name()
            );
            assert!(!prompt.contains("发牌员"));
            assert!(prompt.contains("恩怨小本本"));
        }
    }
}
