//! Pure in-memory offline logistic-regression calibration for L3 weights.

use mahjong_core::{
    project_event, project_view, Match, MatchMode, PlayerAction, PlayerEventKind, PlayerView,
    Ruleset, Seat, SeededWallProvider, Tile, TuidaohuRuleset, WallProvider, WinKind,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use rayon::prelude::*;

use crate::eval::{mix_seed, EvalError};
use crate::l2;
use crate::l3::{
    extract_tenpai_features, extract_wait_features, is_wait_structurally_blocked, OpponentSignal,
};
use crate::l3_weights::{TENPAI_FEATURE_COUNT, WAIT_FEATURE_COUNT};
use crate::scoring::{evaluate_win_with_extra, MeldSummary, ScoringInput, ScoringRules};
use crate::shanten::{compute_remaining, compute_shanten, hand_counts, necessary_tiles_mask};
use crate::shared::{sigmoid, TILE_MASK_34};
use crate::{AiDifficulty, AiStyle, DecisionBudget, DecisionContext, TileEfficiencyBot};

/// Configuration for pure in-memory offline L3 logistic-regression calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrateConfig {
    /// Total self-play hands across L1/L2 bots (`>= 1`).
    pub hands: usize,
    /// Base 64-bit RNG seed.
    pub seed: u64,
    /// Bounded Rayon worker thread count (`>= 1`).
    pub threads: usize,
}

/// Validation metrics for a calibrated binary logistic regression model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogisticMetrics {
    pub train_samples: usize,
    pub val_samples: usize,
    pub val_pos_rate: f64,
    pub val_pred_mean: f64,
    pub val_baseline_log_loss: f64,
    pub val_log_loss: f64,
    pub val_brier: f64,
}

/// Full output of `bench-ai calibrate-l3`.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrateReport {
    pub hands: usize,
    pub seed: u64,
    pub threads: usize,
    pub tenpai_metrics: LogisticMetrics,
    pub wait_metrics: LogisticMetrics,
    pub tenpai_weights: [f64; TENPAI_FEATURE_COUNT],
    pub wait_weights: [f64; WAIT_FEATURE_COUNT],
}

#[derive(Clone, Copy)]
struct Sample<const N: usize> {
    features: [f32; N],
    label: bool,
}

struct HandCalibSamples {
    tenpai: Vec<Sample<TENPAI_FEATURE_COUNT>>,
    wait: Vec<Sample<WAIT_FEATURE_COUNT>>,
}

/// Runs `config.hands` pure in-memory L1/L2 self-play hands, collects true `(tenpai, wait)`
/// observations (skipping `过水胡` and structurally blocked waits), fits L2-regularized
/// logistic regression via Newton-Raphson, and returns validation metrics + weight arrays.
///
/// # Errors
///
/// Returns an [`EvalError`] if `hands == 0`, `threads == 0`, thread-pool setup fails,
/// or core match progression reports an unexpected error.
pub fn run_calibrate_l3(config: &CalibrateConfig) -> Result<CalibrateReport, EvalError> {
    if config.hands == 0 {
        return Err(EvalError::InvalidHands);
    }
    if config.threads == 0 {
        return Err(EvalError::InvalidThreads);
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.threads)
        .build()
        .map_err(|err| EvalError::ThreadPoolBuild(err.to_string()))?;

    let stride = if config.hands <= 500 { 1usize } else { 3usize };
    let collected: Vec<Result<HandCalibSamples, EvalError>> = pool.install(|| {
        (0..config.hands)
            .into_par_iter()
            .map(|hand_idx| collect_hand_samples(config.seed, hand_idx as u64, stride))
            .collect()
    });

    let split_hand = ((config.hands * 4) / 5).clamp(1, config.hands.saturating_sub(1).max(1));
    let mut tenpai_train = Vec::new();
    let mut tenpai_val = Vec::new();
    let mut wait_train = Vec::new();
    let mut wait_val = Vec::new();

    for (hand_idx, res) in collected.into_iter().enumerate() {
        let samples = res?;
        if hand_idx < split_hand {
            tenpai_train.extend(samples.tenpai);
            wait_train.extend(samples.wait);
        } else {
            tenpai_val.extend(samples.tenpai);
            wait_val.extend(samples.wait);
        }
    }

    if tenpai_val.is_empty() {
        tenpai_val.clone_from(&tenpai_train);
    }
    if wait_val.is_empty() {
        wait_val.clone_from(&wait_train);
    }

    let tenpai_reg = 5.0 / tenpai_train.len().max(500) as f64;
    let wait_reg = 5.0 / wait_train.len().max(500) as f64;
    let tenpai_weights = fit_logistic_regression(&tenpai_train, tenpai_reg);
    let wait_weights = fit_logistic_regression(&wait_train, wait_reg);

    let tenpai_metrics = evaluate_logistic(&tenpai_train, &tenpai_val, &tenpai_weights);
    let wait_metrics = evaluate_logistic(&wait_train, &wait_val, &wait_weights);

    Ok(CalibrateReport {
        hands: config.hands,
        seed: config.seed,
        threads: config.threads,
        tenpai_metrics,
        wait_metrics,
        tenpai_weights,
        wait_weights,
    })
}

fn collect_hand_samples(
    seed: u64,
    hand_idx: u64,
    stride: usize,
) -> Result<HandCalibSamples, EvalError> {
    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        SeededWallProvider::new(seed ^ hand_idx.wrapping_mul(0x9E37_79B9_7F4A_7C15)),
    )
    .map_err(|err| EvalError::EngineFailure(err.to_string()))?;

    let rules_profile = TuidaohuRuleset.profile();
    let rules = ScoringRules::from_profile(&rules_profile);
    let allow_quad = rules.allow_four_of_kind_seven_pairs;

    let mut seat_rngs: [ChaCha8Rng; 4] = std::array::from_fn(|seat_idx| {
        ChaCha8Rng::seed_from_u64(mix_seed(seed, hand_idx, 0, seat_idx as u64))
    });
    let l2_ctx = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 64 },
    };

    let mut tenpai = Vec::with_capacity(48);
    let mut wait = Vec::with_capacity(96);
    let mut discard_turn = 0usize;

    loop {
        let mut settled = false;
        let mut progressed = false;

        for seat in Seat::ALL {
            let view = project_view(&game, seat);
            if view.legal_actions.is_empty() {
                continue;
            }

            let can_discard = view
                .legal_actions
                .iter()
                .any(|a| matches!(a, PlayerAction::Discard { .. }));
            let has_win = view.legal_actions.iter().any(|a| {
                matches!(
                    a,
                    PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. }
                )
            });

            if can_discard && !has_win {
                if discard_turn % stride == (hand_idx as usize % stride) {
                    record_turn_observations(
                        &game,
                        &view,
                        allow_quad,
                        rules,
                        &mut tenpai,
                        &mut wait,
                    );
                }
                discard_turn += 1;
            }

            let use_l2 = matches!((hand_idx % 4, seat.index()), (0, _) | (1, 0 | 2) | (2, 0));
            let action = if use_l2 {
                l2::decide(&view, &l2_ctx, &mut seat_rngs[seat.index()])
            } else {
                TileEfficiencyBot::decide(&view, &mut seat_rngs[seat.index()])
            };

            let recorded = game
                .apply_action(seat, action)
                .map_err(|err| EvalError::EngineFailure(err.to_string()))?;
            progressed = true;

            for ev in &recorded {
                if matches!(
                    project_event(ev, seat).kind,
                    PlayerEventKind::HandSettled { .. }
                ) {
                    settled = true;
                    break;
                }
            }
            break;
        }

        if settled {
            break;
        }
        if !progressed {
            return Err(EvalError::EngineFailure(
                "calibration hand stalled before settlement".to_string(),
            ));
        }
    }

    Ok(HandCalibSamples { tenpai, wait })
}

fn record_turn_observations<R: Ruleset, W: WallProvider>(
    game: &Match<R, W>,
    actor_view: &PlayerView,
    allow_quad: bool,
    rules: ScoringRules,
    tenpai_out: &mut Vec<Sample<TENPAI_FEATURE_COUNT>>,
    wait_out: &mut Vec<Sample<WAIT_FEATURE_COUNT>>,
) {
    let actor_counts = hand_counts(actor_view);
    let remaining = compute_remaining(actor_view);
    let ron_in = ScoringInput {
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: actor_view.wall_remaining.max(1),
    };

    for opp in Seat::ALL {
        if opp == actor_view.viewer {
            continue;
        }
        if actor_view.seats[opp.index()].hand_count <= 1 {
            continue;
        }

        let opp_view = project_view(game, opp);
        let opp_counts = hand_counts(&opp_view);
        let opp_melds = MeldSummary::from_meld_views(&opp_view.seats[opp.index()].melds);
        let opp_shanten = compute_shanten(&opp_counts, usize::from(opp_melds.count), allow_quad);

        let mut winning_mask = 0u64;
        if opp_shanten == 0 {
            let (_, nec_mask) =
                necessary_tiles_mask(&opp_counts, usize::from(opp_melds.count), allow_quad);
            let mut bits = nec_mask & TILE_MASK_34;
            while bits != 0 {
                let idx = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
                if evaluate_win_with_extra(&opp_counts, tile, &opp_melds, ron_in, rules).is_some() {
                    winning_mask |= 1u64 << idx;
                }
            }
        }
        let is_tenpai = winning_mask != 0;

        let t_feats = extract_tenpai_features(actor_view, opp);
        tenpai_out.push(Sample {
            features: t_feats.map(|v| v as f32),
            label: is_tenpai,
        });

        if is_tenpai {
            let sig = OpponentSignal::from_view(actor_view, opp);
            for (idx, &cnt) in actor_counts.iter().enumerate() {
                if cnt == 0 {
                    continue;
                }
                let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
                if is_wait_structurally_blocked(actor_view, opp, tile, &remaining, &sig) {
                    continue;
                }
                let w_feats = extract_wait_features(actor_view, opp, tile, &remaining, &sig);
                wait_out.push(Sample {
                    features: w_feats.map(|v| v as f32),
                    label: (winning_mask & (1u64 << idx)) != 0,
                });
            }
        }
    }
}

fn fit_logistic_regression<const N: usize>(samples: &[Sample<N>], l2_reg: f64) -> [f64; N] {
    let mut w = [0.0f64; N];
    if samples.is_empty() {
        return w;
    }

    let pos_count = samples.iter().filter(|s| s.label).count() as f64;
    let p_prior = (pos_count / samples.len() as f64).clamp(1e-4, 1.0 - 1e-4);
    w[0] = (p_prior / (1.0 - p_prior)).ln();

    let n_inv = 1.0 / samples.len() as f64;

    for _iter in 0..30 {
        let mut cur_loss = 0.0f64;
        let mut grad = [0.0f64; N];
        let mut hess = [[0.0f64; N]; N];

        for s in samples {
            let mut z = 0.0f64;
            for j in 0..N {
                z += w[j] * f64::from(s.features[j]);
            }
            let p = sigmoid(z);
            let y = if s.label { 1.0 } else { 0.0 };
            let err = p - y;
            let wt = p * (1.0 - p);

            cur_loss -= y * p.max(1e-15).ln() + (1.0 - y) * (1.0 - p).max(1e-15).ln();

            for j in 0..N {
                let xj = f64::from(s.features[j]);
                grad[j] += err * xj;
                for k in j..N {
                    hess[j][k] += wt * xj * f64::from(s.features[k]);
                }
            }
        }

        cur_loss *= n_inv;
        for (j, g) in grad.iter_mut().enumerate() {
            *g *= n_inv;
            if j > 0 {
                cur_loss += 0.5 * l2_reg * w[j] * w[j];
                *g += l2_reg * w[j];
            }
        }

        for j in 0..N {
            for k in j..N {
                hess[j][k] *= n_inv;
                if j != k {
                    hess[k][j] = hess[j][k];
                }
            }
            hess[j][j] += if j > 0 { l2_reg + 1e-8 } else { 1e-8 };
        }

        let Some(step) = solve_linear_system(&mut hess, &mut grad) else {
            break;
        };

        let mut alpha = 1.0f64;
        let mut accepted = false;
        let mut next_w = w;
        for _ in 0..15 {
            for j in 0..N {
                next_w[j] = w[j] - alpha * step[j];
            }
            let next_loss = logistic_objective(samples, &next_w, l2_reg);
            if next_loss <= cur_loss + 1e-12 {
                accepted = true;
                break;
            }
            alpha *= 0.5;
        }
        if !accepted {
            break;
        }

        let max_delta = step
            .iter()
            .map(|&d| (alpha * d).abs())
            .fold(0.0f64, f64::max);
        w = next_w;
        if max_delta < 1e-7 {
            break;
        }
    }

    w
}

fn logistic_objective<const N: usize>(samples: &[Sample<N>], w: &[f64; N], l2_reg: f64) -> f64 {
    let n_inv = 1.0 / samples.len().max(1) as f64;
    let mut loss = 0.0f64;
    for s in samples {
        let mut z = 0.0f64;
        for j in 0..N {
            z += w[j] * f64::from(s.features[j]);
        }
        let p = sigmoid(z);
        let y = if s.label { 1.0 } else { 0.0 };
        loss -= y * p.max(1e-15).ln() + (1.0 - y) * (1.0 - p).max(1e-15).ln();
    }
    loss *= n_inv;
    for &wj in &w[1..] {
        loss += 0.5 * l2_reg * wj * wj;
    }
    loss
}

fn solve_linear_system<const N: usize>(
    a: &mut [[f64; N]; N],
    b: &mut [f64; N],
) -> Option<[f64; N]> {
    for col in 0..N {
        let mut pivot_row = col;
        let mut max_abs = a[col][col].abs();
        for (row, r_vals) in a.iter().enumerate().skip(col + 1) {
            let v = r_vals[col].abs();
            if v > max_abs {
                max_abs = v;
                pivot_row = row;
            }
        }
        if max_abs < 1e-14 {
            return None;
        }
        if pivot_row != col {
            a.swap(col, pivot_row);
            b.swap(col, pivot_row);
        }

        let pivot = a[col][col];
        for row in (col + 1)..N {
            let factor = a[row][col] / pivot;
            if factor == 0.0 {
                continue;
            }
            for k in col..N {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }

    let mut x = [0.0f64; N];
    for row in (0..N).rev() {
        let mut sum = b[row];
        for k in (row + 1)..N {
            sum -= a[row][k] * x[k];
        }
        x[row] = sum / a[row][row];
    }
    Some(x)
}

fn evaluate_logistic<const N: usize>(
    train: &[Sample<N>],
    val: &[Sample<N>],
    w: &[f64; N],
) -> LogisticMetrics {
    let train_pos = train.iter().filter(|s| s.label).count() as f64;
    let base_p = (train_pos / train.len().max(1) as f64).clamp(1e-6, 1.0 - 1e-6);

    let mut pos_sum = 0.0f64;
    let mut pred_sum = 0.0f64;
    let mut base_ll = 0.0f64;
    let mut mod_ll = 0.0f64;
    let mut brier = 0.0f64;

    for s in val {
        let mut z = 0.0f64;
        for j in 0..N {
            z += w[j] * f64::from(s.features[j]);
        }
        let p = sigmoid(z);
        let y = if s.label { 1.0 } else { 0.0 };
        pos_sum += y;
        pred_sum += p;
        base_ll -= y * base_p.ln() + (1.0 - y) * (1.0 - base_p).ln();
        mod_ll -= y * p.max(1e-15).ln() + (1.0 - y) * (1.0 - p).max(1e-15).ln();
        let diff = p - y;
        brier += diff * diff;
    }

    let n = val.len().max(1) as f64;
    LogisticMetrics {
        train_samples: train.len(),
        val_samples: val.len(),
        val_pos_rate: pos_sum / n,
        val_pred_mean: pred_sum / n,
        val_baseline_log_loss: base_ll / n,
        val_log_loss: mod_ll / n,
        val_brier: brier / n,
    }
}
