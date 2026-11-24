//! Pure in-memory 4-seat duplicate self-play evaluator for `mahjong-ai`.

use core::fmt;
use std::str::FromStr;
use std::time::Instant;

use mahjong_core::{
    project_event, project_view, HandOutcome, Match, MatchMode, PlayerAction, PlayerEventKind,
    PlayerView, Seat, SeededWallProvider, TuidaohuRuleset, WinKind,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use rayon::prelude::*;

pub use crate::calibrate::{run_calibrate_l3, CalibrateConfig, CalibrateReport, LogisticMetrics};
use crate::shared::splitmix64_mix;
use crate::{
    decide, AiDifficulty, AiStyle, DecisionBudget, DecisionContext, RandomBot, TileEfficiencyBot,
};

/// Strategy identifiers supported by the duplicate matchup evaluator and `bench-ai` CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvalStrategy {
    /// Uniformly random legal action baseline (`RandomBot`).
    Random,
    /// Pure L1 shanten + ukeire baseline (`TileEfficiencyBot`).
    TileEfficiency,
    /// `AiDifficulty::Intermediate` via `mahjong_ai::decide`.
    Intermediate,
    /// `AiDifficulty::Strong` without L4 search (placeholder until L4 lands).
    StrongNoL4,
    /// Full `AiDifficulty::Strong` pipeline via `mahjong_ai::decide`.
    Strong,
}

impl EvalStrategy {
    /// Returns the canonical CLI name for this strategy.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::TileEfficiency => "tile-efficiency",
            Self::Intermediate => "intermediate",
            Self::StrongNoL4 => "strong-no-l4",
            Self::Strong => "strong",
        }
    }

    /// Evaluates one decision for `view` using this strategy and `budget`.
    #[must_use]
    pub fn decide<R: RngCore + ?Sized>(
        self,
        view: &PlayerView,
        budget: DecisionBudget,
        rng: &mut R,
    ) -> PlayerAction {
        match self {
            Self::Random => RandomBot::decide(view, rng),
            Self::TileEfficiency => TileEfficiencyBot::decide(view, rng),
            Self::Intermediate => decide(
                view,
                &DecisionContext {
                    difficulty: AiDifficulty::Intermediate,
                    style: AiStyle::Balanced,
                    budget,
                },
                rng,
            ),
            Self::StrongNoL4 => crate::l3::decide_with_options(
                view,
                &DecisionContext {
                    difficulty: AiDifficulty::Strong,
                    style: AiStyle::Balanced,
                    budget,
                },
                false,
                rng,
            ),
            Self::Strong => decide(
                view,
                &DecisionContext {
                    difficulty: AiDifficulty::Strong,
                    style: AiStyle::Balanced,
                    budget,
                },
                rng,
            ),
        }
    }
}

impl fmt::Display for EvalStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EvalStrategy {
    type Err = EvalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "random" => Ok(Self::Random),
            "tile-efficiency" | "tile_efficiency" => Ok(Self::TileEfficiency),
            "intermediate" => Ok(Self::Intermediate),
            "strong-no-l4" | "strong_no_l4" => Ok(Self::StrongNoL4),
            "strong" => Ok(Self::Strong),
            _ => Err(EvalError::UnknownStrategy(s.to_string())),
        }
    }
}

/// Errors produced when configuring or executing an evaluation or calibration run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// Strategy name did not match any known [`EvalStrategy`].
    UnknownStrategy(String),
    /// Duplicate group count was zero.
    InvalidGroups,
    /// Calibration hand count was zero.
    InvalidHands,
    /// Worker thread count was zero.
    InvalidThreads,
    /// Failed to construct the bounded Rayon thread pool.
    ThreadPoolBuild(String),
    /// Core engine match initialization or progression failed unexpectedly.
    EngineFailure(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownStrategy(name) => write!(
                f,
                "unknown strategy '{name}' (expected: random, tile-efficiency, intermediate, strong-no-l4, strong)"
            ),
            Self::InvalidGroups => f.write_str("groups must be >= 1"),
            Self::InvalidHands => f.write_str("hands must be >= 1"),
            Self::InvalidThreads => f.write_str("threads must be >= 1"),
            Self::ThreadPoolBuild(err) => write!(f, "failed to build rayon thread pool: {err}"),
            Self::EngineFailure(err) => write!(f, "match execution failed: {err}"),
        }
    }
}

impl std::error::Error for EvalError {}

/// Returns the default bounded Rayon worker thread count: `min(logical_cpus / 2, 4).max(1)`.
#[must_use]
pub fn default_eval_threads() -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    (cpus / 2).clamp(1, 4)
}

/// Configuration for a 4-seat duplicate matchup evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalConfig {
    /// Candidate strategy rotated across all 4 seats (1 seat per hand).
    pub candidate: EvalStrategy,
    /// Baseline strategy occupying the other 3 seats.
    pub opponent: EvalStrategy,
    /// Number of duplicate deal groups (`total_hands = groups * 4`).
    pub groups: usize,
    /// Base 64-bit seed (`SeededWallProvider::new(seed ^ deal)`).
    pub seed: u64,
    /// Bounded Rayon pool thread count.
    pub threads: usize,
    /// Decision budget passed to strategies using [`crate::decide`].
    pub budget: DecisionBudget,
}

/// Per-step decision latency summary (in milliseconds) for the candidate seat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyQuantiles {
    /// Total candidate decisions timed during `Playing`.
    pub samples: usize,
    /// Arithmetic mean decision latency (ms).
    pub mean_ms: f64,
    /// 50th percentile decision latency (ms).
    pub p50_ms: f64,
    /// 95th percentile decision latency (ms).
    pub p95_ms: f64,
    /// 99th percentile decision latency (ms).
    pub p99_ms: f64,
    /// Maximum single-step decision latency (ms).
    pub max_ms: f64,
}

/// Aggregated statistics from a 4-seat duplicate self-play evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalReport {
    pub candidate: EvalStrategy,
    pub opponent: EvalStrategy,
    pub groups: usize,
    pub total_hands: usize,
    pub threads: usize,
    pub seed: u64,
    pub budget: DecisionBudget,
    /// Paired group mean net score per hand (points/hand).
    pub mean_net_score: f64,
    /// Paired standard error across duplicate groups.
    pub std_err: f64,
    /// Lower bound of the 95% normal-approximation confidence interval (`mean - 1.96 * std_err`).
    pub ci95_low: f64,
    /// Upper bound of the 95% normal-approximation confidence interval (`mean + 1.96 * std_err`).
    pub ci95_high: f64,
    pub tsumo_count: usize,
    pub ron_count: usize,
    pub deal_in_count: usize,
    pub draw_game_count: usize,
    pub tsumo_rate: f64,
    pub ron_rate: f64,
    pub deal_in_rate: f64,
    pub draw_game_rate: f64,
    pub latency: LatencyQuantiles,
}

type ActionTrace = Vec<(u8, u8, PlayerAction)>;

struct GroupResult {
    group_mean_net: f64,
    tsumo_count: usize,
    ron_count: usize,
    deal_in_count: usize,
    draw_game_count: usize,
    latencies_ns: Vec<u64>,
    actions: ActionTrace,
}

/// Hard Slice 3 gate thresholds (Spec #31 §4.4, calibrated to silent 1-pt-base `TuidaohuRuleset`).
pub const GATE_MIN_MEAN_NET_SCORE: f64 = 0.08;
pub const GATE_MIN_DEAL_IN_REDUCTION: f64 = 0.18;
pub const GATE_MAX_REALTIME_P95_MS: f64 = 150.0;
pub const GATE_MAX_REALTIME_MAX_MS: f64 = 200.0;

/// Configuration for the daily 400-hand `bench-ai smoke` check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmokeConfig {
    pub groups: usize,
    pub seed: u64,
    pub threads: usize,
    pub max_samples: usize,
}

impl Default for SmokeConfig {
    fn default() -> Self {
        Self {
            groups: 100,
            seed: 2026,
            threads: default_eval_threads(),
            max_samples: 32,
        }
    }
}

/// Output of `bench-ai smoke`.
#[derive(Debug, Clone, PartialEq)]
pub struct SmokeReport {
    pub report: EvalReport,
    pub passed: bool,
}

/// Cross-thread 1-vs-4 Rayon determinism check output.
#[derive(Debug, Clone, PartialEq)]
pub struct DeterminismReport {
    pub groups: usize,
    pub seed: u64,
    pub max_samples: usize,
    pub action_count: usize,
    pub t1_report: EvalReport,
    pub t4_report: EvalReport,
    pub traces_match: bool,
    pub scores_bit_exact: bool,
    pub passed: bool,
}

/// Configuration for the full 4,000-hand Slice 3 gate (`bench-ai gate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateConfig {
    pub groups: usize,
    pub det_samples: usize,
    pub realtime_groups: usize,
    pub determinism_groups: usize,
    pub seed: u64,
    pub threads: usize,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            groups: 1000,
            det_samples: 128,
            realtime_groups: 20,
            determinism_groups: 25,
            seed: 2026,
            threads: default_eval_threads(),
        }
    }
}

/// Full output and per-gate pass/fail breakdown for `bench-ai gate`.
#[derive(Debug, Clone, PartialEq)]
pub struct GateReport {
    pub config: GateConfig,
    pub random: EvalReport,
    pub tile_efficiency: EvalReport,
    pub intermediate: EvalReport,
    pub strong_no_l4: EvalReport,
    pub strong: EvalReport,
    pub deal_in_reduction: f64,
    pub determinism: DeterminismReport,
    pub realtime: EvalReport,
    pub ablation_monotonic: bool,
    pub threshold_mean_passed: bool,
    pub threshold_ci_passed: bool,
    pub threshold_deal_in_passed: bool,
    pub threshold_passed: bool,
    pub determinism_passed: bool,
    pub realtime_passed: bool,
    pub passed: bool,
}

/// Runs the fast 400-hand `Strong` vs `3 TileEfficiencyBot` smoke gate.
///
/// # Errors
///
/// Returns an [`EvalError`] if matchup parameters are invalid or engine execution fails.
pub fn run_smoke_gate(config: &SmokeConfig) -> Result<SmokeReport, EvalError> {
    let eval_cfg = EvalConfig {
        candidate: EvalStrategy::Strong,
        opponent: EvalStrategy::TileEfficiency,
        groups: config.groups,
        seed: config.seed,
        threads: config.threads,
        budget: DecisionBudget::Deterministic {
            max_samples: config.max_samples,
        },
    };
    let report = run_duplicate_matchup(&eval_cfg)?;
    let passed = report.mean_net_score > 0.0;
    Ok(SmokeReport { report, passed })
}

/// Verifies bit-exact action traces and scores across 1-thread and 4-thread Rayon pools.
///
/// # Errors
///
/// Returns an [`EvalError`] if matchup parameters are invalid or engine execution fails.
pub fn verify_cross_thread_determinism(
    groups: usize,
    seed: u64,
    max_samples: usize,
) -> Result<DeterminismReport, EvalError> {
    let cfg_t1 = EvalConfig {
        candidate: EvalStrategy::Strong,
        opponent: EvalStrategy::TileEfficiency,
        groups,
        seed,
        threads: 1,
        budget: DecisionBudget::Deterministic { max_samples },
    };
    let cfg_t4 = EvalConfig {
        threads: 4,
        ..cfg_t1
    };

    let (t1_report, t1_actions) = run_duplicate_matchup_inner(&cfg_t1, true)?;
    let (t4_report, t4_actions) = run_duplicate_matchup_inner(&cfg_t4, true)?;

    let traces_match = t1_actions == t4_actions;
    let scores_bit_exact = t1_report.mean_net_score.to_bits() == t4_report.mean_net_score.to_bits()
        && t1_report.std_err.to_bits() == t4_report.std_err.to_bits()
        && t1_report.ci95_low.to_bits() == t4_report.ci95_low.to_bits()
        && t1_report.ci95_high.to_bits() == t4_report.ci95_high.to_bits()
        && t1_report.tsumo_count == t4_report.tsumo_count
        && t1_report.ron_count == t4_report.ron_count
        && t1_report.deal_in_count == t4_report.deal_in_count
        && t1_report.draw_game_count == t4_report.draw_game_count;
    let passed = traces_match && scores_bit_exact;

    Ok(DeterminismReport {
        groups,
        seed,
        max_samples,
        action_count: t1_actions.len(),
        t1_report,
        t4_report,
        traces_match,
        scores_bit_exact,
        passed,
    })
}

/// Runs all 4 Slice 3 quantitative hard gates and returns the full report.
///
/// # Errors
///
/// Returns an [`EvalError`] if any matchup parameter is invalid or engine execution fails.
pub fn run_gate(config: &GateConfig) -> Result<GateReport, EvalError> {
    if config.realtime_groups == 0 || config.determinism_groups == 0 {
        return Err(EvalError::InvalidGroups);
    }

    let det_budget = DecisionBudget::Deterministic {
        max_samples: config.det_samples,
    };
    let make_cfg = |candidate: EvalStrategy| EvalConfig {
        candidate,
        opponent: EvalStrategy::TileEfficiency,
        groups: config.groups,
        seed: config.seed,
        threads: config.threads,
        budget: det_budget,
    };

    let random = run_duplicate_matchup(&make_cfg(EvalStrategy::Random))?;
    let tile_efficiency = run_duplicate_matchup(&make_cfg(EvalStrategy::TileEfficiency))?;
    let intermediate = run_duplicate_matchup(&make_cfg(EvalStrategy::Intermediate))?;
    let strong_no_l4 = run_duplicate_matchup(&make_cfg(EvalStrategy::StrongNoL4))?;
    let strong = run_duplicate_matchup(&make_cfg(EvalStrategy::Strong))?;

    let ablation_monotonic = random.mean_net_score < tile_efficiency.mean_net_score
        && tile_efficiency.mean_net_score < intermediate.mean_net_score
        && intermediate.mean_net_score < strong_no_l4.mean_net_score
        && strong_no_l4.mean_net_score <= strong.mean_net_score;

    let deal_in_reduction = if tile_efficiency.deal_in_rate > 0.0 {
        (tile_efficiency.deal_in_rate - strong.deal_in_rate) / tile_efficiency.deal_in_rate
    } else {
        0.0
    };

    let threshold_mean_passed = strong.mean_net_score >= GATE_MIN_MEAN_NET_SCORE;
    let threshold_ci_passed = strong.ci95_low > 0.0;
    let threshold_deal_in_passed = deal_in_reduction >= GATE_MIN_DEAL_IN_REDUCTION;
    let threshold_passed = threshold_mean_passed && threshold_ci_passed && threshold_deal_in_passed;

    let determinism = verify_cross_thread_determinism(
        config.determinism_groups,
        config.seed,
        config.det_samples,
    )?;
    let determinism_passed = determinism.passed;

    let realtime = run_duplicate_matchup(&EvalConfig {
        candidate: EvalStrategy::Strong,
        opponent: EvalStrategy::TileEfficiency,
        groups: config.realtime_groups,
        seed: config.seed,
        threads: config.threads,
        budget: DecisionBudget::default(),
    })?;
    let realtime_passed = realtime.total_hands >= 60
        && realtime.latency.p95_ms <= GATE_MAX_REALTIME_P95_MS
        && realtime.latency.max_ms <= GATE_MAX_REALTIME_MAX_MS;

    let passed = ablation_monotonic && threshold_passed && determinism_passed && realtime_passed;

    Ok(GateReport {
        config: *config,
        random,
        tile_efficiency,
        intermediate,
        strong_no_l4,
        strong,
        deal_in_reduction,
        determinism,
        realtime,
        ablation_monotonic,
        threshold_mean_passed,
        threshold_ci_passed,
        threshold_deal_in_passed,
        threshold_passed,
        determinism_passed,
        realtime_passed,
        passed,
    })
}

/// Runs a 4-seat duplicate rotation matchup in memory using a bounded Rayon thread pool
/// and deterministic index-ordered reduction.
///
/// # Errors
///
/// Returns an [`EvalError`] if `groups == 0`, `threads == 0`, thread-pool initialization fails,
/// or core match execution reports an unexpected error.
pub fn run_duplicate_matchup(config: &EvalConfig) -> Result<EvalReport, EvalError> {
    let (report, _) = run_duplicate_matchup_inner(config, false)?;
    Ok(report)
}

fn run_duplicate_matchup_inner(
    config: &EvalConfig,
    record_trace: bool,
) -> Result<(EvalReport, ActionTrace), EvalError> {
    if config.groups == 0 {
        return Err(EvalError::InvalidGroups);
    }
    if config.threads == 0 {
        return Err(EvalError::InvalidThreads);
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.threads)
        .build()
        .map_err(|err| EvalError::ThreadPoolBuild(err.to_string()))?;

    let groups: Vec<Result<GroupResult, EvalError>> = pool.install(|| {
        if matches!(config.budget, DecisionBudget::Realtime { .. }) {
            // Measure single-decision Realtime latency without outer group work-stealing.
            (0..config.groups)
                .map(|deal| evaluate_group(config, deal as u64, record_trace))
                .collect()
        } else {
            (0..config.groups)
                .into_par_iter()
                .map(|deal| evaluate_group(config, deal as u64, record_trace))
                .collect()
        }
    });

    let mut group_means = Vec::with_capacity(config.groups);
    let mut tsumo_count = 0usize;
    let mut ron_count = 0usize;
    let mut deal_in_count = 0usize;
    let mut draw_game_count = 0usize;
    let mut all_latencies_ns = Vec::new();
    let mut all_actions = Vec::new();

    for group_res in groups {
        let group = group_res?;
        group_means.push(group.group_mean_net);
        tsumo_count += group.tsumo_count;
        ron_count += group.ron_count;
        deal_in_count += group.deal_in_count;
        draw_game_count += group.draw_game_count;
        all_latencies_ns.extend(group.latencies_ns);
        if record_trace {
            all_actions.extend(group.actions);
        }
    }

    let n = group_means.len() as f64;
    let mean_net_score = group_means.iter().sum::<f64>() / n;
    let std_err = if group_means.len() >= 2 {
        let variance = group_means
            .iter()
            .map(|&x| {
                let d = x - mean_net_score;
                d * d
            })
            .sum::<f64>()
            / (group_means.len() - 1) as f64;
        (variance / n).sqrt()
    } else {
        0.0
    };
    let half_ci = 1.96 * std_err;

    let total_hands = config.groups * 4;
    let total_hands_f = total_hands as f64;
    let latency = summarize_latencies(&mut all_latencies_ns);

    Ok((
        EvalReport {
            candidate: config.candidate,
            opponent: config.opponent,
            groups: config.groups,
            total_hands,
            threads: config.threads,
            seed: config.seed,
            budget: config.budget,
            mean_net_score,
            std_err,
            ci95_low: mean_net_score - half_ci,
            ci95_high: mean_net_score + half_ci,
            tsumo_count,
            ron_count,
            deal_in_count,
            draw_game_count,
            tsumo_rate: tsumo_count as f64 / total_hands_f,
            ron_rate: ron_count as f64 / total_hands_f,
            deal_in_rate: deal_in_count as f64 / total_hands_f,
            draw_game_rate: draw_game_count as f64 / total_hands_f,
            latency,
        },
        all_actions,
    ))
}

fn evaluate_group(
    config: &EvalConfig,
    deal: u64,
    record_trace: bool,
) -> Result<GroupResult, EvalError> {
    let deal_seed = config.seed ^ deal;
    let mut sum_net = 0i64;
    let mut tsumo_count = 0usize;
    let mut ron_count = 0usize;
    let mut deal_in_count = 0usize;
    let mut draw_game_count = 0usize;
    let mut latencies_ns = Vec::with_capacity(160);
    let mut actions = Vec::new();

    for (rot, &candidate_seat) in Seat::ALL.iter().enumerate() {
        let mut game = Match::new(
            MatchMode::FreeTable,
            TuidaohuRuleset,
            SeededWallProvider::new(deal_seed),
        )
        .map_err(|err| EvalError::EngineFailure(err.to_string()))?;

        let mut seat_rngs: [ChaCha8Rng; 4] = std::array::from_fn(|seat_idx| {
            ChaCha8Rng::seed_from_u64(mix_seed(config.seed, deal, rot as u64, seat_idx as u64))
        });

        let settlement = loop {
            let mut settled = None;
            let mut progressed = false;

            for seat in Seat::ALL {
                let view = project_view(&game, seat);
                if view.legal_actions.is_empty() {
                    continue;
                }
                let is_candidate = seat == candidate_seat;
                let strategy = if is_candidate {
                    config.candidate
                } else {
                    config.opponent
                };

                let start = is_candidate.then(Instant::now);
                let action = strategy.decide(&view, config.budget, &mut seat_rngs[seat.index()]);
                if let Some(t0) = start {
                    let elapsed = t0.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                    latencies_ns.push(elapsed);
                }
                if record_trace {
                    actions.push((rot as u8, seat.index() as u8, action));
                }

                let recorded = game
                    .apply_action(seat, action)
                    .map_err(|err| EvalError::EngineFailure(err.to_string()))?;
                progressed = true;

                for ev in &recorded {
                    if let PlayerEventKind::HandSettled { settlement } =
                        project_event(ev, candidate_seat).kind
                    {
                        settled = Some(settlement);
                        break;
                    }
                }
                break;
            }

            if let Some(s) = settled {
                break s;
            }
            if !progressed {
                return Err(EvalError::EngineFailure(
                    "hand stalled with no legal actions before settlement".to_string(),
                ));
            }
        };

        sum_net += i64::from(settlement.score_deltas[candidate_seat.index()]);
        match &settlement.outcome {
            HandOutcome::Win {
                winners, discarder, ..
            } => {
                if winners
                    .iter()
                    .any(|w| w.seat == candidate_seat && w.win_kind == WinKind::Tsumo)
                {
                    tsumo_count += 1;
                }
                if winners.iter().any(|w| {
                    w.seat == candidate_seat
                        && matches!(w.win_kind, WinKind::Ron | WinKind::RobKongHu)
                }) {
                    ron_count += 1;
                }
                if *discarder == Some(candidate_seat) {
                    deal_in_count += 1;
                }
            }
            HandOutcome::DrawGame => {
                draw_game_count += 1;
            }
        }
    }

    Ok(GroupResult {
        group_mean_net: sum_net as f64 / 4.0,
        tsumo_count,
        ron_count,
        deal_in_count,
        draw_game_count,
        latencies_ns,
        actions,
    })
}

fn summarize_latencies(latencies_ns: &mut [u64]) -> LatencyQuantiles {
    let samples = latencies_ns.len();
    if samples == 0 {
        return LatencyQuantiles {
            samples: 0,
            mean_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            max_ms: 0.0,
        };
    }

    latencies_ns.sort_unstable();
    let total_ns: u128 = latencies_ns.iter().map(|&v| u128::from(v)).sum();
    let mean_ms = (total_ns as f64 / samples as f64) / 1_000_000.0;
    let ns_to_ms = |ns: u64| ns as f64 / 1_000_000.0;
    let quantile = |pct: usize| -> f64 {
        let idx = (samples * pct)
            .div_ceil(100)
            .saturating_sub(1)
            .min(samples - 1);
        ns_to_ms(latencies_ns[idx])
    };

    LatencyQuantiles {
        samples,
        mean_ms,
        p50_ms: quantile(50),
        p95_ms: quantile(95),
        p99_ms: quantile(99),
        max_ms: ns_to_ms(latencies_ns[samples - 1]),
    }
}

pub(crate) fn mix_seed(seed: u64, deal: u64, rot: u64, seat_idx: u64) -> u64 {
    splitmix64_mix(
        seed ^ deal.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ rot.wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ seat_idx.wrapping_mul(0x94D0_49BB_1331_11EB),
    )
}
