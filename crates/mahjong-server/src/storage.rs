//! Pure Rust JSONL history and atomic `preferences.json` persistence (`std::fs` + `serde_json`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mahjong_core::{HandOutcome, WinKind};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::history_dto::{
    ClearHistoryResponse, HandRecord, HistoryExportResponse, MatchRecord, PlayerStats,
    PreferencesData, StatsResponse, HISTORY_SCHEMA_VERSION,
};
use crate::protocol::{LobbyConfig, SeatKind, TablePace};

const PREFERENCES_FILE: &str = "preferences.json";
const HANDS_FILE: &str = "hands.jsonl";
const MATCHES_FILE: &str = "matches.jsonl";
const MAX_RECENT_NICKNAMES: usize = 12;
const RECENT_MATCHES_LIMIT: usize = 50;
const RECENT_HANDS_LIMIT: usize = 100;

static ATOMIC_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Returns the current Unix epoch timestamp in milliseconds.
#[must_use]
pub fn current_unix_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

/// Resolves the platform default data directory from explicit environment values.
#[must_use]
pub fn resolve_default_data_dir_with_env(
    is_macos: bool,
    xdg_data_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> PathBuf {
    let non_empty = |v: Option<&OsStr>| v.filter(|s| !s.is_empty()).map(PathBuf::from);
    if is_macos {
        if let Some(home_dir) = non_empty(home) {
            return home_dir
                .join("Library")
                .join("Application Support")
                .join("majjj");
        }
    } else {
        if let Some(xdg_dir) = non_empty(xdg_data_home) {
            return xdg_dir.join("majjj");
        }
        if let Some(home_dir) = non_empty(home) {
            return home_dir.join(".local").join("share").join("majjj");
        }
    }
    std::env::temp_dir().join("majjj")
}

/// Resolves the platform default data directory (`~/Library/Application Support/majjj` on macOS,
/// `$XDG_DATA_HOME/majjj` or `~/.local/share/majjj` on Linux).
#[must_use]
pub fn resolve_default_data_dir() -> PathBuf {
    resolve_default_data_dir_with_env(
        cfg!(target_os = "macos"),
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// Resolves the active data directory from an optional explicit override (`--data-dir` / `MAJJJ_DATA_DIR`).
#[must_use]
pub fn resolve_data_dir(explicit: Option<PathBuf>) -> PathBuf {
    match explicit {
        Some(path) if !path.as_os_str().is_empty() => path,
        _ => resolve_default_data_dir(),
    }
}

/// Normalizes a player nickname by trimming leading and trailing whitespace.
#[must_use]
pub fn normalize_nickname(raw: &str) -> String {
    raw.trim().to_string()
}

fn atomic_write_bytes(target: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = target.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let seq = ATOMIC_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("tmp");
    let tmp_path = parent.join(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));

    let write_res = (|| -> Result<(), std::io::Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    if let Err(err) = fs::rename(&tmp_path, target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

fn load_jsonl_tolerant<T: DeserializeOwned + Serialize>(
    path: &Path,
) -> Result<Vec<T>, std::io::Error> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let raw_lines: Vec<&[u8]> = bytes
        .split(|&b| b == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .collect();
    let total_lines = raw_lines.len();
    let mut records = Vec::with_capacity(total_lines);
    let mut needs_repair = !bytes.ends_with(b"\n");

    for (idx, raw_line) in raw_lines.into_iter().enumerate() {
        let is_last = idx + 1 == total_lines;
        let line_str = match std::str::from_utf8(raw_line) {
            Ok(s) => s.trim_end_matches('\r'),
            Err(err) => {
                eprintln!(
                    "warning: skipping invalid UTF-8 line {} in {}: {err}",
                    idx + 1,
                    path.display()
                );
                needs_repair = true;
                if is_last {
                    break;
                }
                continue;
            }
        };
        match serde_json::from_str::<T>(line_str) {
            Ok(record) => records.push(record),
            Err(err) => {
                if is_last {
                    eprintln!(
                        "warning: skipping truncated trailing line in {}: {err}",
                        path.display()
                    );
                    needs_repair = true;
                    break;
                }
                eprintln!(
                    "warning: skipping corrupt line {} in {}: {err}",
                    idx + 1,
                    path.display()
                );
                needs_repair = true;
            }
        }
    }

    // Rewrite clean valid lines if a truncated tail or missing trailing newline was found,
    // so subsequent O(1) appends never concatenate onto a partial line.
    if needs_repair {
        write_jsonl_atomic(path, &records)?;
    }

    Ok(records)
}

fn append_jsonl_line<T: Serialize>(path: &Path, record: &T) -> Result<(), std::io::Error> {
    let mut line = serde_json::to_string(record).map_err(std::io::Error::other)?;
    line.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    file.sync_data()?;
    Ok(())
}

fn write_jsonl_atomic<T: Serialize>(path: &Path, records: &[T]) -> Result<(), std::io::Error> {
    let mut buf = Vec::new();
    for record in records {
        serde_json::to_writer(&mut buf, record).map_err(std::io::Error::other)?;
        buf.push(b'\n');
    }
    atomic_write_bytes(path, &buf)
}

/// Loads `preferences.json` from `data_dir` with default fallback on missing or corrupt files.
///
/// # Errors
///
/// Returns an I/O error if `preferences.json` exists but cannot be read.
pub(crate) fn load_preferences_for_dir(data_dir: &Path) -> Result<PreferencesData, std::io::Error> {
    load_preferences(&data_dir.join(PREFERENCES_FILE))
}

fn load_preferences(path: &Path) -> Result<PreferencesData, std::io::Error> {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<PreferencesData>(&contents) {
            Ok(mut prefs) => {
                prefs.schema_version = HISTORY_SCHEMA_VERSION;
                let mut deduped = Vec::new();
                for raw in prefs.recent_nicknames {
                    let norm = normalize_nickname(&raw);
                    if !norm.is_empty() && !deduped.contains(&norm) {
                        deduped.push(norm);
                    }
                }
                deduped.truncate(MAX_RECENT_NICKNAMES);
                prefs.recent_nicknames = deduped;
                Ok(prefs)
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to parse {}: {err}; using defaults",
                    path.display()
                );
                Ok(PreferencesData::default())
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(PreferencesData::default()),
        Err(err) => Err(err),
    }
}

/// Pure Rust local history and preferences store.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    data_dir: Option<PathBuf>,
    preferences: PreferencesData,
    hands: Vec<HandRecord>,
    matches: Vec<MatchRecord>,
}

impl HistoryStore {
    /// Creates an ephemeral in-memory store (used by default in unit/integration tests unless `data_dir` is set).
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            data_dir: None,
            preferences: PreferencesData::default(),
            hands: Vec::new(),
            matches: Vec::new(),
        }
    }

    /// Opens or creates `data_dir`, loading `preferences.json` and JSONL history files.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if `data_dir` cannot be created or read.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let preferences = load_preferences(&dir.join(PREFERENCES_FILE))?;
        let hands = load_jsonl_tolerant::<HandRecord>(&dir.join(HANDS_FILE))?;
        let matches = load_jsonl_tolerant::<MatchRecord>(&dir.join(MATCHES_FILE))?;
        Ok(Self {
            data_dir: Some(dir),
            preferences,
            hands,
            matches,
        })
    }

    /// Returns the loaded/current preferences.
    #[must_use]
    pub fn preferences(&self) -> &PreferencesData {
        &self.preferences
    }

    /// Appends one settled hand record to memory and `hands.jsonl` (`sync_data()`).
    pub fn append_hand(&mut self, hand: HandRecord) {
        if let Some(dir) = &self.data_dir {
            let path = dir.join(HANDS_FILE);
            if let Err(err) = append_jsonl_line(&path, &hand) {
                eprintln!("warning: failed to append {}: {err}", path.display());
            }
        }
        self.hands.push(hand);
    }

    /// Appends one completed match summary record to memory and `matches.jsonl` (`sync_data()`).
    pub fn append_match(&mut self, summary: MatchRecord) {
        if let Some(dir) = &self.data_dir {
            let path = dir.join(MATCHES_FILE);
            if let Err(err) = append_jsonl_line(&path, &summary) {
                eprintln!("warning: failed to append {}: {err}", path.display());
            }
        }
        self.matches.push(summary);
    }

    /// Marks all settled hands belonging to `match_id` as `aborted: true` and rewrites `hands.jsonl` atomically.
    pub fn mark_match_aborted(&mut self, match_id: &str) {
        let mut changed = false;
        for hand in &mut self.hands {
            if hand.match_id == match_id && !hand.aborted {
                hand.aborted = true;
                changed = true;
            }
        }
        if !changed {
            return;
        }
        if let Some(dir) = &self.data_dir {
            let path = dir.join(HANDS_FILE);
            if let Err(err) = write_jsonl_atomic(&path, &self.hands) {
                eprintln!(
                    "warning: failed to update aborted hands in {}: {err}",
                    path.display()
                );
            }
        }
    }

    /// Computes per-normalized-nickname player statistics and recent match/hand details.
    #[must_use]
    pub fn stats(&self) -> StatsResponse {
        #[derive(Default)]
        struct Accumulator {
            completed_matches: u32,
            settled_hands: u32,
            aborted_hands: u32,
            wins: u32,
            tsumo_wins: u32,
            deal_ins: u32,
            ai_assisted_hands: u32,
            cumulative_net_score: i64,
            completed_match_net_score: i64,
        }

        let mut by_nickname: BTreeMap<String, Accumulator> = BTreeMap::new();
        let mut matches_with_hands: HashSet<&str> = HashSet::new();
        let mut per_match_seat_player_net: HashMap<(&str, usize, String), i64> = HashMap::new();

        for hand in &self.hands {
            matches_with_hands.insert(hand.match_id.as_str());
            let discarder = match &hand.settlement.outcome {
                HandOutcome::Win { discarder, .. } => *discarder,
                HandOutcome::DrawGame => None,
            };
            for (idx, seat_rec) in hand.seats.iter().enumerate() {
                if seat_rec.seat_kind != SeatKind::Human {
                    continue;
                }
                let norm = normalize_nickname(&seat_rec.nickname);
                if norm.is_empty() {
                    continue;
                }
                let delta = i64::from(hand.settlement.score_deltas[idx]);
                *per_match_seat_player_net
                    .entry((hand.match_id.as_str(), idx, norm.clone()))
                    .or_default() += delta;
                let acc = by_nickname.entry(norm).or_default();
                acc.settled_hands += 1;
                if hand.aborted {
                    acc.aborted_hands += 1;
                }
                if seat_rec.assisted_by_ai {
                    acc.ai_assisted_hands += 1;
                }
                acc.cumulative_net_score += delta;

                if let HandOutcome::Win { winners, .. } = &hand.settlement.outcome {
                    for winner in winners {
                        if winner.seat == seat_rec.seat {
                            acc.wins += 1;
                            if winner.win_kind == WinKind::Tsumo {
                                acc.tsumo_wins += 1;
                            }
                        }
                    }
                    if discarder == Some(seat_rec.seat) {
                        acc.deal_ins += 1;
                    }
                }
            }
        }

        for m in &self.matches {
            let has_hand_records = matches_with_hands.contains(m.match_id.as_str());
            for seat_rec in &m.seats {
                if seat_rec.seat_kind != SeatKind::Human {
                    continue;
                }
                let norm = normalize_nickname(&seat_rec.nickname);
                if norm.is_empty() {
                    continue;
                }
                let match_net = if has_hand_records {
                    per_match_seat_player_net
                        .get(&(m.match_id.as_str(), seat_rec.seat.index(), norm.clone()))
                        .copied()
                        .unwrap_or(0)
                } else {
                    i64::from(seat_rec.net_delta)
                };
                let acc = by_nickname.entry(norm).or_default();
                acc.completed_matches += 1;
                acc.completed_match_net_score += match_net;
            }
        }

        let mut players: Vec<PlayerStats> = by_nickname
            .into_iter()
            .map(|(nickname, acc)| {
                let average_net_score = if acc.completed_matches > 0 {
                    acc.completed_match_net_score as f64 / f64::from(acc.completed_matches)
                } else {
                    0.0
                };
                let average_hand_net_score = if acc.settled_hands > 0 {
                    acc.cumulative_net_score as f64 / f64::from(acc.settled_hands)
                } else {
                    0.0
                };
                PlayerStats {
                    nickname,
                    completed_matches: acc.completed_matches,
                    settled_hands: acc.settled_hands,
                    aborted_hands: acc.aborted_hands,
                    wins: acc.wins,
                    tsumo_wins: acc.tsumo_wins,
                    deal_ins: acc.deal_ins,
                    ai_assisted_hands: acc.ai_assisted_hands,
                    cumulative_net_score: acc.cumulative_net_score,
                    average_net_score,
                    average_hand_net_score,
                }
            })
            .collect();

        players.sort_by(|a, b| {
            b.completed_matches
                .cmp(&a.completed_matches)
                .then_with(|| b.settled_hands.cmp(&a.settled_hands))
                .then_with(|| b.cumulative_net_score.cmp(&a.cumulative_net_score))
                .then_with(|| a.nickname.cmp(&b.nickname))
        });

        let recent_matches = self
            .matches
            .iter()
            .rev()
            .take(RECENT_MATCHES_LIMIT)
            .cloned()
            .collect();
        let recent_hands = self
            .hands
            .iter()
            .rev()
            .take(RECENT_HANDS_LIMIT)
            .cloned()
            .collect();

        StatsResponse {
            schema_version: HISTORY_SCHEMA_VERSION,
            total_matches: u32::try_from(self.matches.len()).unwrap_or(u32::MAX),
            total_hands: u32::try_from(self.hands.len()).unwrap_or(u32::MAX),
            players,
            recent_matches,
            recent_hands,
        }
    }

    /// Builds a complete JSON backup payload containing preferences, matches, and hands.
    #[must_use]
    pub fn export(&self) -> HistoryExportResponse {
        HistoryExportResponse {
            schema_version: HISTORY_SCHEMA_VERSION,
            exported_at_ms: current_unix_ms(),
            preferences: self.preferences.clone(),
            matches: self.matches.clone(),
            hands: self.hands.clone(),
        }
    }

    /// Clears `hands.jsonl` and `matches.jsonl` while preserving `preferences.json`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if clearing the JSONL files fails on disk.
    pub fn clear_history(&mut self) -> Result<ClearHistoryResponse, std::io::Error> {
        let cleared_matches = u32::try_from(self.matches.len()).unwrap_or(u32::MAX);
        let cleared_hands = u32::try_from(self.hands.len()).unwrap_or(u32::MAX);
        if let Some(dir) = &self.data_dir {
            atomic_write_bytes(&dir.join(HANDS_FILE), b"")?;
            atomic_write_bytes(&dir.join(MATCHES_FILE), b"")?;
        }
        self.hands.clear();
        self.matches.clear();
        Ok(ClearHistoryResponse {
            cleared: true,
            cleared_matches,
            cleared_hands,
        })
    }

    /// Updates default lobby configuration and persists `preferences.json` atomically if changed.
    pub fn record_lobby_config(&mut self, config: LobbyConfig) {
        if self.preferences.default_mode == config.mode
            && self.preferences.default_seating == config.seating
            && self.preferences.default_ai_difficulty == config.ai_difficulty
            && self.preferences.default_pace == config.pace
        {
            return;
        }
        self.preferences.default_mode = config.mode;
        self.preferences.default_seating = config.seating;
        self.preferences.default_ai_difficulty = config.ai_difficulty;
        self.preferences.default_pace = config.pace;
        self.save_preferences();
    }

    /// Updates default table pace and persists `preferences.json` atomically if changed.
    pub fn record_pace(&mut self, pace: TablePace) {
        if self.preferences.default_pace == pace {
            return;
        }
        self.preferences.default_pace = pace;
        self.save_preferences();
    }

    /// Updates the slow-think nudge toggle and persists `preferences.json` atomically if changed.
    pub fn record_slow_think_nudge(&mut self, enabled: bool) {
        if self.preferences.slow_think_nudge_enabled == enabled {
            return;
        }
        self.preferences.slow_think_nudge_enabled = enabled;
        self.save_preferences();
    }

    /// Records a normalized non-empty nickname at the front of `recent_nicknames` and persists if changed.
    pub fn record_nickname(&mut self, raw_nickname: &str) {
        let normalized = normalize_nickname(raw_nickname);
        if normalized.is_empty() {
            return;
        }
        if self
            .preferences
            .recent_nicknames
            .first()
            .is_some_and(|first| first == &normalized)
        {
            return;
        }
        self.preferences
            .recent_nicknames
            .retain(|existing| existing != &normalized);
        self.preferences.recent_nicknames.insert(0, normalized);
        self.preferences
            .recent_nicknames
            .truncate(MAX_RECENT_NICKNAMES);
        self.save_preferences();
    }

    fn save_preferences(&self) {
        let Some(dir) = &self.data_dir else {
            return;
        };
        let path = dir.join(PREFERENCES_FILE);
        match serde_json::to_vec_pretty(&self.preferences) {
            Ok(mut bytes) => {
                bytes.push(b'\n');
                if let Err(err) = atomic_write_bytes(&path, &bytes) {
                    eprintln!("warning: failed to persist {}: {err}", path.display());
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to serialize preferences for {}: {err}",
                    path.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use mahjong_ai::AiDifficulty;
    use mahjong_core::MatchMode;

    use super::{resolve_data_dir, resolve_default_data_dir_with_env, HistoryStore};
    use crate::history_dto::HISTORY_SCHEMA_VERSION;
    use crate::protocol::{LobbyConfig, SeatingRule, TablePace};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let id = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), id));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn resolves_platform_default_data_dirs_and_explicit_override() {
        assert_eq!(
            resolve_default_data_dir_with_env(true, None, Some(OsStr::new("/Users/alice"))),
            PathBuf::from("/Users/alice/Library/Application Support/majjj")
        );
        assert_eq!(
            resolve_default_data_dir_with_env(
                false,
                Some(OsStr::new("/var/lib/xdg")),
                Some(OsStr::new("/home/alice")),
            ),
            PathBuf::from("/var/lib/xdg/majjj")
        );
        assert_eq!(
            resolve_default_data_dir_with_env(
                false,
                Some(OsStr::new("")),
                Some(OsStr::new("/home/alice")),
            ),
            PathBuf::from("/home/alice/.local/share/majjj")
        );
        assert_eq!(
            resolve_data_dir(Some(PathBuf::from("/tmp/custom-majjj"))),
            PathBuf::from("/tmp/custom-majjj")
        );
    }

    #[test]
    fn open_creates_missing_dir_and_recovers_corrupt_preferences() {
        let root = TempDir::new("majjj_storage_prefs");
        let nested = root.path.join("nested").join("data");
        assert!(!nested.exists());

        let mut store = HistoryStore::open(&nested).expect("open creates missing dirs");
        assert!(nested.is_dir());
        assert_eq!(store.preferences().schema_version, HISTORY_SCHEMA_VERSION);

        store.record_lobby_config(LobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Intermediate,
            pace: TablePace::Fast,
        });
        store.record_nickname("  Alice  ");
        store.record_nickname("Bob");
        store.record_nickname("Alice");
        assert!(store.preferences().slow_think_nudge_enabled);
        store.record_slow_think_nudge(false);

        let reopened = HistoryStore::open(&nested).expect("reopen persisted preferences");
        assert_eq!(reopened.preferences().default_mode, MatchMode::FreeTable);
        assert_eq!(
            reopened.preferences().default_seating,
            SeatingRule::FixedOpposite
        );
        assert_eq!(
            reopened.preferences().default_ai_difficulty,
            AiDifficulty::Intermediate
        );
        assert_eq!(reopened.preferences().default_pace, TablePace::Fast);
        assert!(!reopened.preferences().slow_think_nudge_enabled);
        assert_eq!(
            reopened.preferences().recent_nicknames,
            vec!["Alice".to_string(), "Bob".to_string()]
        );

        // Legacy preferences.json lacking slow_think_nudge_enabled defaults cleanly to true.
        fs::write(
            nested.join("preferences.json"),
            r#"{"schema_version":1,"recent_nicknames":[],"default_mode":"EastOnly","default_seating":"KeepLobbySeats","default_ai_difficulty":"Strong","default_pace":"Standard"}"#,
        )
        .expect("write legacy prefs");
        let legacy = HistoryStore::open(&nested).expect("legacy prefs should default nudge true");
        assert!(legacy.preferences().slow_think_nudge_enabled);

        // Corrupt preferences.json falls back cleanly without failing startup.
        fs::write(nested.join("preferences.json"), "{corrupt").expect("write corrupt prefs");
        let recovered = HistoryStore::open(&nested).expect("corrupt prefs should not fail open");
        assert_eq!(recovered.preferences().default_mode, MatchMode::EastOnly);
        assert!(recovered.preferences().slow_think_nudge_enabled);
        assert!(recovered.preferences().recent_nicknames.is_empty());
    }

    fn sample_settlement(
        winner: Option<(mahjong_core::Seat, mahjong_core::WinKind)>,
        discarder: Option<mahjong_core::Seat>,
        score_deltas: [i32; 4],
        new_scores: [i32; 4],
    ) -> mahjong_core::HandSettlement {
        use mahjong_core::{
            HandOutcome, HandSettlement, RevealedHand, ScoreItem, Seat, Tile, Wind,
            WinnerSettlement,
        };
        let outcome = match winner {
            Some((seat, win_kind)) => HandOutcome::Win {
                winners: vec![WinnerSettlement {
                    seat,
                    win_kind,
                    items: vec![ScoreItem {
                        code: "tsumo".to_string(),
                        label: "自摸".to_string(),
                        fan: 1,
                    }],
                    total_fan: 1,
                    base_points: 2,
                }],
                discarder,
                winning_tile: Tile::new(0).expect("valid tile"),
            },
            None => HandOutcome::DrawGame,
        };
        HandSettlement {
            wind_round: Wind::East,
            hand_number: 1,
            dealer_streak: 0,
            dealer: Seat::East,
            outcome,
            kong_transfers: Vec::new(),
            score_deltas,
            new_scores,
            revealed_hands: Seat::ALL.map(|seat| RevealedHand {
                seat,
                closed_tiles: Vec::new(),
                drawn_tile: None,
                melds: Vec::new(),
            }),
        }
    }

    fn sample_hand(
        match_id: &str,
        hand_index: u32,
        mode: MatchMode,
        settlement: mahjong_core::HandSettlement,
        east: (&str, bool),
        south: (crate::protocol::SeatKind, &str),
    ) -> crate::history_dto::HandRecord {
        use crate::history_dto::{HandRecord, HandSeatRecord};
        use crate::protocol::SeatKind;
        use mahjong_core::Seat;
        let (east_name, east_assisted) = east;
        let (south_kind, south_name) = south;
        HandRecord {
            schema_version: HISTORY_SCHEMA_VERSION,
            match_id: match_id.to_string(),
            hand_index,
            timestamp_ms: 1_700_000_000_000 + u64::from(hand_index),
            mode,
            settlement,
            seats: [
                HandSeatRecord {
                    seat: Seat::East,
                    nickname: east_name.to_string(),
                    seat_kind: SeatKind::Human,
                    assisted_by_ai: east_assisted,
                    took_over_ai: false,
                },
                HandSeatRecord {
                    seat: Seat::South,
                    nickname: south_name.to_string(),
                    seat_kind: south_kind,
                    assisted_by_ai: false,
                    took_over_ai: false,
                },
                HandSeatRecord {
                    seat: Seat::West,
                    nickname: "AI 2".to_string(),
                    seat_kind: SeatKind::Ai,
                    assisted_by_ai: false,
                    took_over_ai: false,
                },
                HandSeatRecord {
                    seat: Seat::North,
                    nickname: "AI 3".to_string(),
                    seat_kind: SeatKind::Ai,
                    assisted_by_ai: false,
                    took_over_ai: false,
                },
            ],
            aborted: false,
        }
    }

    #[test]
    fn jsonl_persistence_truncation_recovery_abort_stats_export_and_clear() {
        use std::io::Write;

        use crate::history_dto::{MatchRecord, MatchSeatRecord};
        use crate::protocol::SeatKind;
        use mahjong_core::{Seat, WinKind};

        let dir = TempDir::new("majjj_storage_jsonl");
        let mut store = HistoryStore::open(&dir.path).expect("open store");
        store.record_lobby_config(LobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Intermediate,
            pace: TablePace::Fast,
        });

        // Hand 1 (match-1): East ("  Alice  ") wins by Tsumo (+6), AI assisted. South is AI 1.
        let h1 = sample_hand(
            "match-1",
            1,
            MatchMode::FreeTable,
            sample_settlement(
                Some((Seat::East, WinKind::Tsumo)),
                None,
                [6, -2, -2, -2],
                [6, -2, -2, -2],
            ),
            ("  Alice  ", true),
            (SeatKind::Ai, "AI 1"),
        );
        store.append_hand(h1);

        // Hand 2 (match-1): South taken over by Bob; Alice deals in to Bob (+4 / -4).
        let h2 = sample_hand(
            "match-1",
            2,
            MatchMode::FreeTable,
            sample_settlement(
                Some((Seat::South, WinKind::Ron)),
                Some(Seat::East),
                [-4, 4, 0, 0],
                [2, 2, -2, -2],
            ),
            ("Alice", false),
            (SeatKind::Human, " Bob "),
        );
        store.append_hand(h2);

        store.append_match(MatchRecord {
            schema_version: HISTORY_SCHEMA_VERSION,
            match_id: "match-1".to_string(),
            timestamp_ms: 1_700_000_000_010,
            mode: MatchMode::FreeTable,
            hands_played: 2,
            initial_scores: [0, 0, 0, 0],
            final_scores: [2, 2, -2, -2],
            net_deltas: [2, 2, -2, -2],
            seats: [
                MatchSeatRecord {
                    seat: Seat::East,
                    nickname: "Alice".to_string(),
                    seat_kind: SeatKind::Human,
                    assisted_by_ai: true,
                    final_score: 2,
                    net_delta: 2,
                },
                MatchSeatRecord {
                    seat: Seat::South,
                    nickname: "Bob".to_string(),
                    seat_kind: SeatKind::Human,
                    assisted_by_ai: false,
                    final_score: 2,
                    net_delta: 2,
                },
                MatchSeatRecord {
                    seat: Seat::West,
                    nickname: "AI 2".to_string(),
                    seat_kind: SeatKind::Ai,
                    assisted_by_ai: false,
                    final_score: -2,
                    net_delta: -2,
                },
                MatchSeatRecord {
                    seat: Seat::North,
                    nickname: "AI 3".to_string(),
                    seat_kind: SeatKind::Ai,
                    assisted_by_ai: false,
                    final_score: -2,
                    net_delta: -2,
                },
            ],
        });

        // Aborted EastOnly match-2 with 1 settled hand -> marked aborted: true on disk.
        let h_abort = sample_hand(
            "match-2",
            1,
            MatchMode::EastOnly,
            sample_settlement(None, None, [0, 0, 0, 0], [0, 0, 0, 0]),
            ("Alice", false),
            (SeatKind::Ai, "AI 1"),
        );
        store.append_hand(h_abort);
        store.mark_match_aborted("match-2");

        // Simulate power loss leaving truncated trailing JSON lines in both files.
        for filename in ["hands.jsonl", "matches.jsonl"] {
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(dir.path.join(filename))
                .expect("open jsonl for truncation");
            f.write_all(br#"{"schema_version":1,"match_id":"broken"#)
                .expect("write truncated tail");
        }

        let reopened = HistoryStore::open(&dir.path).expect("reopen tolerates truncated tail");
        let stats = reopened.stats();
        assert_eq!(stats.total_matches, 1);
        assert_eq!(stats.total_hands, 3);

        // Human players only ("AI 1/2/3" excluded), normalized nicknames merged.
        assert_eq!(stats.players.len(), 2);
        let alice = stats
            .players
            .iter()
            .find(|p| p.nickname == "Alice")
            .expect("Alice stats");
        assert_eq!(alice.completed_matches, 1);
        assert_eq!(alice.settled_hands, 3);
        assert_eq!(alice.aborted_hands, 1);
        assert_eq!(alice.wins, 1);
        assert_eq!(alice.tsumo_wins, 1);
        assert_eq!(alice.deal_ins, 1);
        assert_eq!(alice.ai_assisted_hands, 1);
        assert_eq!(alice.cumulative_net_score, 2);
        assert!((alice.average_net_score - 2.0).abs() < 1e-6);

        let bob = stats
            .players
            .iter()
            .find(|p| p.nickname == "Bob")
            .expect("Bob stats");
        assert_eq!(bob.completed_matches, 1);
        assert_eq!(bob.settled_hands, 1); // Hand 1 belonged to AI 1, Hand 2 to Bob
        assert_eq!(bob.wins, 1);
        assert_eq!(bob.tsumo_wins, 0);
        assert_eq!(bob.cumulative_net_score, 4);
        assert!((bob.average_net_score - 4.0).abs() < 1e-6);
        assert!((bob.average_hand_net_score - 4.0).abs() < 1e-6);

        let aborted_row = stats
            .recent_hands
            .iter()
            .find(|h| h.match_id == "match-2")
            .expect("aborted hand retained");
        assert!(aborted_row.aborted);

        let exported = reopened.export();
        assert_eq!(exported.matches.len(), 1);
        assert_eq!(exported.hands.len(), 3);
        assert_eq!(exported.preferences.default_mode, MatchMode::FreeTable);

        // Clear removes hands & matches, preserves preferences.json.
        let mut reopened_mut = reopened;
        let cleared = reopened_mut.clear_history().expect("clear history");
        assert!(cleared.cleared);
        assert_eq!(cleared.cleared_matches, 1);
        assert_eq!(cleared.cleared_hands, 3);

        let after_clear = HistoryStore::open(&dir.path).expect("reopen after clear");
        assert_eq!(after_clear.stats().total_matches, 0);
        assert_eq!(after_clear.stats().total_hands, 0);
        assert_eq!(after_clear.preferences().default_mode, MatchMode::FreeTable);
    }

    #[test]
    fn default_seating_is_keep_lobby_seats_and_legacy_preferences_deserialize_cleanly() {
        use crate::history_dto::PreferencesData;

        assert_eq!(LobbyConfig::default().seating, SeatingRule::KeepLobbySeats);
        assert_eq!(
            PreferencesData::default().default_seating,
            SeatingRule::KeepLobbySeats
        );

        let root = TempDir::new("majjj_storage_legacy_seating");
        let fresh = HistoryStore::open(&root.path).expect("open fresh store");
        assert_eq!(
            fresh.preferences().default_seating,
            SeatingRule::KeepLobbySeats
        );
        drop(fresh);

        for (raw_rule, expected_rule) in [
            ("Random", SeatingRule::Random),
            ("FixedOpposite", SeatingRule::FixedOpposite),
            ("FixedAdjacent", SeatingRule::FixedAdjacent),
        ] {
            let legacy_json = format!(
                r#"{{
                    "schema_version": 1,
                    "recent_nicknames": ["Alice"],
                    "default_mode": "EastOnly",
                    "default_seating": "{raw_rule}",
                    "default_ai_difficulty": "Strong",
                    "default_pace": "Standard"
                }}"#
            );
            fs::write(root.path.join("preferences.json"), legacy_json)
                .expect("write legacy preferences");
            let mut store =
                HistoryStore::open(&root.path).expect("legacy preferences should load cleanly");
            assert_eq!(store.preferences().default_seating, expected_rule);

            store.record_nickname("Bob");
            let reopened = HistoryStore::open(&root.path)
                .expect("saved preferences after loading legacy should reopen cleanly");
            assert_eq!(reopened.preferences().default_seating, expected_rule);
            assert_eq!(
                reopened.preferences().recent_nicknames,
                vec!["Bob".to_string(), "Alice".to_string()]
            );
        }
    }
}
