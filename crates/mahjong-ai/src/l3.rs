//! Layer 3 (L3): Non-Riichi opponent threat, exact `过水胡` 0-risk window,
//! Kabe combinatorial wait estimation, calibrated logistic risk scoring,
//! and difficulty/style-parameterized offense-defense synthesis.

use mahjong_core::{MeldView, PlayerAction, PlayerView, Seat, Suit, Tile};
use rand_core::RngCore;

use crate::l2::{
    evaluate_candidates, evaluate_pon_with_scorer, fast_path_action, select_best_candidate,
    CandidateBuffer, CandidateEv, RobKongRiskModel,
};
use crate::l3_weights::{TENPAI_FEATURE_COUNT, TENPAI_WEIGHTS, WAIT_FEATURE_COUNT, WAIT_WEIGHTS};
use crate::scoring::MeldSummary;
use crate::shanten::compute_remaining;
use crate::{AiDifficulty, AiStyle, DecisionContext};

const INITIAL_WALL_TILES: f64 = 83.0;
const TERMINAL_OR_HONOR_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

/// Exact remaining unseen wait combinations around a candidate tile from `remaining[0..34]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KabeWaitCombos {
    /// Two-sided sequence pairs (`23m` for `1m`/`4m`, etc.).
    pub ryanmen: u16,
    /// Closed inside sequence pairs (`13m` for `2m`, etc.).
    pub kanchan: u16,
    /// Edge sequence pairs (`12m` for `3m`, `89m` for `7m`).
    pub penchan: u16,
    /// Unseen pairs for a `shanpon` dual-triplet wait (`C(rem[t], 2)`).
    pub shanpon: u16,
    /// Unseen single copies for a `tanki` pair wait (`rem[t]`).
    pub tanki: u16,
}

impl KabeWaitCombos {
    /// Sum of all standard wait combinations.
    #[must_use]
    pub const fn total_std_combos(self) -> u16 {
        self.ryanmen + self.kanchan + self.penchan + self.shanpon + self.tanki
    }
}

/// Static difficulty and play-style parameters driving L1-L3 utility synthesis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct L3Profile {
    /// Multiplier applied to expected point loss from discarding into opponent waits.
    pub risk_weight: f64,
    /// Multiplier applied to expected `抢杠胡` loss on `AddedKong`.
    pub rob_kong_risk_multiplier: f64,
    /// Penalty multiplier for shrinking closed-hand safety when melding (`Pon` / `ExposedKong`).
    pub meld_shrink_weight: f64,
    /// Additive bonus per unit of `p_win` (`AggressiveFast` favors quick wins).
    pub speed_bias: f64,
    /// Additive bonus scaling extra win points above 2 pts (`DefensiveBigHand` chases big hands).
    pub big_hand_bias: f64,
    /// Additive preference bias on calling `Pon` / `ExposedKong`.
    pub meld_bias: f64,
    /// Reserved toggle for Ticket #36 L4 search (`Strong` vs `StrongNoL4`).
    pub l4_enabled: bool,
}

impl L3Profile {
    /// Derives the static profile for `(ctx.difficulty, ctx.style, l4_enabled)`.
    #[must_use]
    pub const fn from_context(ctx: &DecisionContext, l4_enabled: bool) -> Self {
        let (base_risk, base_rob, base_shrink, style_scale) = match ctx.difficulty {
            AiDifficulty::Strong => (1.00, 1.25, 0.90, 1.00),
            AiDifficulty::Intermediate => (0.40, 0.46, 0.32, 0.45),
        };
        let (risk_mult, speed_bias, big_hand_bias, meld_bias) = match ctx.style {
            AiStyle::AggressiveFast => (0.55, 0.45, -0.08, 0.25),
            AiStyle::Balanced => (1.00, 0.00, 0.00, 0.00),
            AiStyle::DefensiveBigHand => (1.65, -0.18, 0.28, -0.25),
        };
        Self {
            risk_weight: base_risk * risk_mult,
            rob_kong_risk_multiplier: base_rob * risk_mult,
            meld_shrink_weight: base_shrink * risk_mult,
            speed_bias: speed_bias * style_scale,
            big_hand_bias: big_hand_bias * style_scale,
            meld_bias: meld_bias * style_scale,
            l4_enabled,
        }
    }
}

/// Publicly observable opponent threat and hand-shape signals extracted from `PlayerView`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpponentSignal {
    /// Opponent seat.
    pub seat: Seat,
    /// Estimated probability `P(Tenpai_opp)` in `[0.0, 1.0]`.
    pub p_tenpai: f64,
    /// Whether `门前清` is still intact (no `Pon`, `ExposedKong`, `AddedKong`, or `Chi`).
    pub is_menqing: bool,
    /// Number of declared melds (`0..=4`).
    pub melds_count: usize,
    /// Flush concentration signal `[Man, Bamboo, Dot]` in `[0.0, 1.0]`.
    pub flush_concentration: [f64; 3],
    /// Affinity for honor tiles under suspected `混一色` / `字一色`.
    pub honor_flush_affinity: f64,
    /// Estimated `字一色` probability in `[0.0, 1.0]`.
    pub all_honors_prob: f64,
    /// River discard counts `[Man, Bamboo, Dot]`.
    pub river_suit_counts: [u8; 3],
    /// River honor discard count.
    pub river_honor_count: u8,
    /// Number of honor melds declared by this opponent.
    pub meld_honor_count: u8,
    /// Number of distinct number suits present in this opponent's melds.
    pub active_meld_num_suits: u8,
}

impl OpponentSignal {
    /// Summarizes public signals for `opp_seat` from `view`.
    #[must_use]
    pub fn from_view(view: &PlayerView, opp_seat: Seat) -> Self {
        let seat_view = &view.seats[opp_seat.index()];
        let melds_summary = MeldSummary::from_meld_views(&seat_view.melds);

        let mut meld_suit_has = [false; 3];
        let mut meld_honor_count = 0u8;
        for meld in &seat_view.melds {
            match *meld {
                MeldView::Pon { tile, .. }
                | MeldView::ExposedKong { tile, .. }
                | MeldView::AddedKong { tile, .. } => {
                    if let Some(s) = number_suit_index(tile) {
                        meld_suit_has[s] = true;
                    } else {
                        meld_honor_count = meld_honor_count.saturating_add(1);
                    }
                }
                MeldView::ConcealedKong { tile: Some(t), .. } => {
                    if let Some(s) = number_suit_index(t) {
                        meld_suit_has[s] = true;
                    } else {
                        meld_honor_count = meld_honor_count.saturating_add(1);
                    }
                }
                MeldView::ConcealedKong { tile: None, .. } => {}
                MeldView::Chi { tiles, .. } => {
                    if let Some(s) = number_suit_index(tiles[0]) {
                        meld_suit_has[s] = true;
                    }
                }
            }
        }
        let active_meld_num_suits =
            u8::from(meld_suit_has[0]) + u8::from(meld_suit_has[1]) + u8::from(meld_suit_has[2]);

        let mut river_suit_counts = [0u8; 3];
        let mut river_honor_count = 0u8;
        for d in &seat_view.river {
            if let Some(s) = number_suit_index(d.tile) {
                river_suit_counts[s] = river_suit_counts[s].saturating_add(1);
            } else {
                river_honor_count = river_honor_count.saturating_add(1);
            }
        }
        let total_num_river = river_suit_counts[0] + river_suit_counts[1] + river_suit_counts[2];
        let river_len = seat_view.river.len();

        let mut flush_concentration = [0.0f64; 3];
        let mut honor_flush_affinity = 0.0f64;
        let mut all_honors_prob = 0.0f64;

        if active_meld_num_suits == 1 {
            let ms = meld_suit_has
                .iter()
                .position(|&has| has)
                .expect("exactly 1 active meld suit");
            let own = f64::from(river_suit_counts[ms]);
            let other = f64::from(total_num_river - river_suit_counts[ms]);
            let suit_melds = seat_view
                .melds
                .iter()
                .filter(|m| match **m {
                    MeldView::Pon { tile, .. }
                    | MeldView::ExposedKong { tile, .. }
                    | MeldView::AddedKong { tile, .. } => number_suit_index(tile) == Some(ms),
                    MeldView::ConcealedKong { tile: Some(t), .. } => {
                        number_suit_index(t) == Some(ms)
                    }
                    _ => false,
                })
                .count();
            let genuine_flush = (suit_melds >= 2 && own <= 1.0 && other >= 2.0)
                || (other >= 3.0 && own == 0.0)
                || (other >= 5.0 && own <= 1.0);
            if genuine_flush {
                let purity = (other - 1.5 * own).max(0.0);
                let meld_bonus = if suit_melds >= 2 { 0.35 } else { 0.15 };
                let conc = (meld_bonus + purity / 6.0).clamp(0.0, 1.0);
                flush_concentration[ms] = conc;
                if meld_honor_count > 0 || river_honor_count <= 1 {
                    honor_flush_affinity = 0.65 * conc;
                }
            }
        } else if active_meld_num_suits == 0 {
            if meld_honor_count > 0 {
                for s in 0..3 {
                    let own = f64::from(river_suit_counts[s]);
                    let other = f64::from(total_num_river - river_suit_counts[s]);
                    if (other >= 2.0 && own == 0.0) || (other >= 4.0 && own <= 1.0) {
                        flush_concentration[s] = ((other - 1.5 * own) / 6.0).clamp(0.0, 0.90);
                    }
                }
                let max_suit_conc = flush_concentration.iter().copied().fold(0.0f64, f64::max);
                let distinct_river_suits = river_suit_counts.iter().filter(|&&c| c > 0).count();
                if meld_honor_count >= 2 && total_num_river >= 3 && distinct_river_suits >= 2 {
                    all_honors_prob = ((f64::from(total_num_river) - 1.0) / 8.0).clamp(0.20, 0.70);
                } else if distinct_river_suits == 3 && river_honor_count == 0 {
                    all_honors_prob = 0.35;
                }
                if river_honor_count <= 1 {
                    honor_flush_affinity = max_suit_conc
                        .max(0.35 * f64::from(meld_honor_count).min(2.0) / 2.0)
                        .max(all_honors_prob);
                }
            } else if river_len >= 5 {
                for s in 0..3 {
                    let own = f64::from(river_suit_counts[s]);
                    let other = f64::from(total_num_river - river_suit_counts[s]);
                    if (other >= 4.0 && own == 0.0) || (other >= 6.0 && own <= 1.0) {
                        flush_concentration[s] = ((other - 2.0 * own - 2.0) / 6.0).clamp(0.0, 0.85);
                    }
                }
                let max_suit_conc = flush_concentration.iter().copied().fold(0.0f64, f64::max);
                if river_honor_count <= 1 && max_suit_conc > 0.0 {
                    honor_flush_affinity = 0.35 * max_suit_conc;
                }
                if river_len >= 6
                    && river_honor_count == 0
                    && river_suit_counts.iter().all(|&c| c >= 1)
                {
                    let ah = ((f64::from(total_num_river) - 3.0) / 8.0).clamp(0.0, 0.70);
                    all_honors_prob = ah * 0.45;
                    honor_flush_affinity = honor_flush_affinity.max(ah);
                }
            }
        }

        let p_tenpai = opponent_p_tenpai(view, opp_seat);
        Self {
            seat: opp_seat,
            p_tenpai,
            is_menqing: melds_summary.is_menqing,
            melds_count: usize::from(melds_summary.count),
            flush_concentration,
            honor_flush_affinity,
            all_honors_prob,
            river_suit_counts,
            river_honor_count,
            meld_honor_count,
            active_meld_num_suits,
        }
    }
}

/// Returns `true` iff any seat other than `opp_seat` discarded `tile` strictly after
/// `opp_seat.last_draw_or_meld_seq`, guaranteeing `P(Wait_opp(tile)) == 0.0` via `过水胡`.
///
/// This 0-risk lock automatically expires as soon as `opp_seat` draws a tile or claims a meld
/// (which advances `last_draw_or_meld_seq`).
#[must_use]
pub fn is_passed_hu_zero_risk(view: &PlayerView, opp_seat: Seat, tile: Tile) -> bool {
    let opp_seq = view.seats[opp_seat.index()].last_draw_or_meld_seq;
    view.seats.iter().any(|seat_view| {
        seat_view.seat != opp_seat
            && seat_view
                .river
                .iter()
                .any(|d| d.tile == tile && d.seq > opp_seq)
    })
}

/// Computes exact Kabe remaining unseen wait combinations around `tile` using `remaining[0..34]`.
///
/// For number tiles, `remaining[tile] == 0` zeroes `shanpon` and `tanki` ONLY, leaving
/// sequence waits (`ryanmen`, `kanchan`, `penchan`) open when adjacent tiles remain unseen.
/// For honors, `remaining[tile] == 0` blocks all five standard wait forms.
#[must_use]
pub fn kabe_wait_combos(tile: Tile, remaining: &[u8; Tile::COUNT]) -> KabeWaitCombos {
    let idx = tile.index();
    let r_self = u16::from(remaining[idx]);
    let shanpon = r_self * r_self.saturating_sub(1) / 2;
    let tanki = r_self;

    if idx >= 27 {
        return KabeWaitCombos {
            ryanmen: 0,
            kanchan: 0,
            penchan: 0,
            shanpon,
            tanki,
        };
    }

    let base = (idx / 9) * 9;
    let r = idx % 9;
    let rem = |offset: usize| u16::from(remaining[base + offset]);

    let mut ryanmen = 0u16;
    let mut kanchan = 0u16;
    let mut penchan = 0u16;

    if r >= 2 {
        let lower_prod = rem(r - 2) * rem(r - 1);
        if r == 2 {
            penchan += lower_prod;
        } else {
            ryanmen += lower_prod;
        }
    }
    if (1..=7).contains(&r) {
        kanchan += rem(r - 1) * rem(r + 1);
    }
    if r <= 6 {
        let upper_prod = rem(r + 1) * rem(r + 2);
        if r == 6 {
            penchan += upper_prod;
        } else {
            ryanmen += upper_prod;
        }
    }

    KabeWaitCombos {
        ryanmen,
        kanchan,
        penchan,
        shanpon,
        tanki,
    }
}

/// Extracts the 14 public features used by `P(Tenpai_opp)`.
#[must_use]
pub fn extract_tenpai_features(view: &PlayerView, opp_seat: Seat) -> [f64; TENPAI_FEATURE_COUNT] {
    let seat_view = &view.seats[opp_seat.index()];
    let progress = ((INITIAL_WALL_TILES - f64::from(view.wall_remaining)) / INITIAL_WALL_TILES)
        .clamp(0.0, 1.0);
    let melds = seat_view.melds.len();
    let river = &seat_view.river;
    let river_len = river.len();

    let trailing_tsumogiri = river.iter().rev().take_while(|d| d.is_tsumogiri).count();
    let tsumogiri_norm = (trailing_tsumogiri as f64 / 6.0).min(1.0);

    let mut mid_late_tedashi_count = 0usize;
    let mut recent_middle_tedashi = 0.0f64;
    for (i, d) in river.iter().enumerate() {
        if i >= 4 && !d.is_tsumogiri && is_middle_tile(d.tile) {
            mid_late_tedashi_count += 1;
            let recency = 1.0 / (1.0 + (river_len - 1 - i) as f64);
            if recency > recent_middle_tedashi {
                recent_middle_tedashi = recency;
            }
        }
    }

    let early_len = river_len.min(4);
    let early_middle_ratio = if early_len == 0 {
        0.0
    } else {
        let early_mid = river[..early_len]
            .iter()
            .filter(|d| is_middle_tile(d.tile))
            .count();
        early_mid as f64 / early_len as f64
    };

    [
        1.0,
        progress,
        progress * progress,
        if melds >= 1 { 1.0 } else { 0.0 },
        if melds >= 2 { 1.0 } else { 0.0 },
        if melds >= 3 { 1.0 } else { 0.0 },
        if melds >= 4 { 1.0 } else { 0.0 },
        (river_len as f64 / 18.0).min(1.5),
        tsumogiri_norm,
        progress * tsumogiri_norm,
        (mid_late_tedashi_count as f64 / 4.0).min(1.0),
        recent_middle_tedashi,
        early_middle_ratio,
        (f64::from(seat_view.hand_count) / 13.0).clamp(0.0, 1.1),
    ]
}

/// Extracts the 17 public features used by `P(Wait_opp(tile) | Tenpai_opp)`.
#[must_use]
pub fn extract_wait_features(
    view: &PlayerView,
    opp_seat: Seat,
    tile: Tile,
    remaining: &[u8; Tile::COUNT],
    sig: &OpponentSignal,
) -> [f64; WAIT_FEATURE_COUNT] {
    let combos = kabe_wait_combos(tile, remaining);
    let river = &view.seats[opp_seat.index()].river;
    let recent_start = river.len().saturating_sub(4);

    let in_recent = river[recent_start..].iter().any(|d| d.tile == tile);
    let in_early = !in_recent && river[..recent_start].iter().any(|d| d.tile == tile);

    let (zone_honor, zone_terminal, zone_2_8, zone_3_7) = if tile.is_honor() {
        (1.0, 0.0, 0.0, 0.0)
    } else if tile.is_terminal() {
        (0.0, 1.0, 0.0, 0.0)
    } else if matches!(tile.rank(), Some(2 | 8)) {
        (0.0, 0.0, 1.0, 0.0)
    } else if matches!(tile.rank(), Some(3 | 7)) {
        (0.0, 0.0, 0.0, 1.0)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let weak_suji = compute_weak_suji(tile, river);

    let (flush_suit_match, off_flush_suit) = if let Some(s) = number_suit_index(tile) {
        let own = sig.flush_concentration[s];
        let other = sig.flush_concentration[(s + 1) % 3].max(sig.flush_concentration[(s + 2) % 3]);
        (own, other)
    } else {
        (sig.honor_flush_affinity, 0.0)
    };

    let multi_pon_shanpon_tanki =
        if sig.melds_count >= 2 && (combos.shanpon > 0 || combos.tanki > 0) {
            let meld_scale = (sig.melds_count.saturating_sub(1) as f64).clamp(1.0, 3.0) / 3.0;
            let st_score =
                0.6 * (f64::from(combos.shanpon) / 6.0) + 0.4 * (f64::from(combos.tanki) / 4.0);
            meld_scale * st_score
        } else {
            0.0
        };

    [
        1.0,
        (f64::from(combos.ryanmen) / 32.0).min(1.0),
        (f64::from(combos.kanchan) / 16.0).min(1.0),
        (f64::from(combos.penchan) / 16.0).min(1.0),
        (f64::from(combos.shanpon) / 6.0).min(1.0),
        (f64::from(combos.tanki) / 4.0).min(1.0),
        zone_honor,
        zone_terminal,
        zone_2_8,
        zone_3_7,
        if in_recent { 1.0 } else { 0.0 },
        if in_early { 1.0 } else { 0.0 },
        weak_suji,
        flush_suit_match,
        off_flush_suit,
        multi_pon_shanpon_tanki,
        if combos.total_std_combos() == 0 {
            1.0
        } else {
            0.0
        },
    ]
}

/// Estimates `P(Tenpai_opp)` from public state using static calibrated logistic weights.
#[must_use]
pub fn opponent_p_tenpai(view: &PlayerView, opp_seat: Seat) -> f64 {
    if view.seats[opp_seat.index()].hand_count <= 1 {
        return 1.0;
    }
    let feats = extract_tenpai_features(view, opp_seat);
    sigmoid(dot(&TENPAI_WEIGHTS, &feats))
}

/// Returns `true` iff `tile` is mathematically blocked from all possible waits against `opp_seat`
/// (either via `过水胡` or zero standard wait combinations when `十三幺` is also impossible).
#[must_use]
pub fn is_wait_structurally_blocked(
    view: &PlayerView,
    opp_seat: Seat,
    tile: Tile,
    remaining: &[u8; Tile::COUNT],
    sig: &OpponentSignal,
) -> bool {
    if is_passed_hu_zero_risk(view, opp_seat, tile) {
        return true;
    }
    let combos = kabe_wait_combos(tile, remaining);
    combos.total_std_combos() == 0
        && (sig.melds_count > 0
            || !tile.is_terminal_or_honor()
            || any_other_orphan_dead(remaining, tile))
}

/// Estimates `P(Wait_opp(tile) | Tenpai_opp)` with exact `过水胡` and structural dead-wait gates.
#[must_use]
pub fn opponent_p_wait_given_tenpai(
    view: &PlayerView,
    opp_seat: Seat,
    tile: Tile,
    remaining: &[u8; Tile::COUNT],
    sig: &OpponentSignal,
) -> f64 {
    if is_wait_structurally_blocked(view, opp_seat, tile, remaining, sig) {
        return 0.0;
    }

    let feats = extract_wait_features(view, opp_seat, tile, remaining, sig);
    sigmoid(dot(&WAIT_WEIGHTS, &feats))
}

/// Estimates expected `Ron` payout `E[Score_opp(tile)]` (`1.0` open / `2.0` `门前清`,
/// scaled for flush/`字一色`, multi-`Pon` `碰碰胡`, and `河底捞鱼`, capped at `fan_cap`).
#[must_use]
pub fn opponent_expected_ron_score(
    view: &PlayerView,
    _opp_seat: Seat,
    tile: Tile,
    remaining: &[u8; Tile::COUNT],
    sig: &OpponentSignal,
) -> f64 {
    let fan_cap = f64::from(view.rule_profile.fan_cap);
    let base_fan = if sig.is_menqing { 1.0 } else { 0.0 };
    let last_tile_fan = if view.wall_remaining == 0 { 1.0 } else { 0.0 };

    let (p_half, p_full, p_honors) = if let Some(s) = number_suit_index(tile) {
        let conc = sig.flush_concentration[s];
        let p_full = if sig.meld_honor_count == 0 && conc > 0.15 {
            let honor_shed_boost = if sig.river_honor_count >= 1 {
                0.85
            } else {
                0.55
            };
            (conc * honor_shed_boost).clamp(0.0, 0.90)
        } else {
            0.0
        };
        let p_half = if conc > 0.15 {
            (conc * 0.90 - p_full).max(0.0)
        } else {
            0.0
        };
        (p_half, p_full, 0.0)
    } else {
        let p_honors = sig.all_honors_prob.clamp(0.0, 0.80);
        let p_half = (sig.honor_flush_affinity - p_honors)
            .max(0.0)
            .clamp(0.0, 0.85);
        (p_half, 0.0, p_honors)
    };
    let p_none = (1.0 - p_half - p_full - p_honors).max(0.0);

    let p_toitoi = if remaining[tile.index()] == 0 && sig.melds_count < 4 {
        0.0
    } else {
        match sig.melds_count {
            0 | 1 => 0.0,
            2 => 0.25,
            3 => 0.60,
            _ => 0.95,
        }
    };

    let payout_for = |extra_fan: f64| -> f64 {
        let capped = (base_fan + last_tile_fan + extra_fan).min(fan_cap);
        2.0_f64.powf(capped)
    };

    let mut expected = 0.0f64;
    for (p_f, f_fan) in [(p_none, 0.0), (p_half, 1.0), (p_full, 2.0), (p_honors, 4.0)] {
        if p_f <= 0.0 {
            continue;
        }
        expected += p_f * (1.0 - p_toitoi) * payout_for(f_fan);
        if p_toitoi > 0.0 {
            expected += p_f * p_toitoi * payout_for(f_fan + 1.0);
        }
    }
    expected
}

/// Precomputed per-opponent signals and per-tile deal-in risks for a single `PlayerView`.
#[derive(Debug, Clone)]
pub struct L3DefenseModel {
    profile: L3Profile,
    remaining: [u8; Tile::COUNT],
    signals: [OpponentSignal; 4],
    viewer: Seat,
    tile_risks: [f64; Tile::COUNT],
    max_payout: f64,
}

impl L3DefenseModel {
    /// Builds the L3 risk model strictly from public `view` and `profile`.
    #[must_use]
    pub fn new(view: &PlayerView, profile: L3Profile) -> Self {
        let remaining = compute_remaining(view);
        let signals = Seat::ALL.map(|seat| OpponentSignal::from_view(view, seat));
        let max_payout = 2.0_f64.powf(f64::from(view.rule_profile.fan_cap));
        let mut tile_risks = [0.0f64; Tile::COUNT];

        for (idx, risk_slot) in tile_risks.iter_mut().enumerate() {
            let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
            let mut sum = 0.0f64;
            for opp in Seat::ALL {
                if opp == view.viewer {
                    continue;
                }
                let sig = &signals[opp.index()];
                let p_wait = opponent_p_wait_given_tenpai(view, opp, tile, &remaining, sig);
                if p_wait > 0.0 {
                    let e_score = opponent_expected_ron_score(view, opp, tile, &remaining, sig);
                    sum += sig.p_tenpai * p_wait * e_score;
                }
            }
            *risk_slot = sum;
        }

        Self {
            profile,
            remaining,
            signals,
            viewer: view.viewer,
            tile_risks,
            max_payout,
        }
    }

    /// Returns the total expected deal-in loss (`pts`) across all 3 opponents for discarding `tile`.
    #[must_use]
    pub fn discard_risk(&self, tile: Tile) -> f64 {
        self.tile_risks[tile.index()]
    }

    /// Returns the estimated `OpponentSignal` for `seat`.
    #[must_use]
    pub fn opponent_signal(&self, seat: Seat) -> &OpponentSignal {
        &self.signals[seat.index()]
    }

    /// Returns aggregate opponent tenpai threat (`sum_{opp} P(Tenpai_opp) * base_multiplier`).
    #[must_use]
    pub fn total_opponent_threat(&self) -> f64 {
        let mut total = 0.0f64;
        for opp in Seat::ALL {
            if opp == self.viewer {
                continue;
            }
            let sig = &self.signals[opp.index()];
            let base = if sig.is_menqing { 2.0 } else { 1.0 };
            let flush_peak = sig
                .flush_concentration
                .iter()
                .copied()
                .fold(0.0f64, f64::max)
                .max(sig.all_honors_prob);
            let amp = 1.0 + 1.5 * flush_peak + if sig.melds_count >= 2 { 0.4 } else { 0.0 };
            total += sig.p_tenpai * base * amp;
        }
        total
    }

    /// Returns the maximum estimated `P(Tenpai_opp)` among the three opponents.
    #[must_use]
    pub fn max_opponent_tenpai(&self) -> f64 {
        let mut peak = 0.0f64;
        for opp in Seat::ALL {
            if opp != self.viewer {
                peak = peak.max(self.signals[opp.index()].p_tenpai);
            }
        }
        peak
    }

    /// Returns `P(at least one opponent is tenpai) = 1 - prod_{opp}(1 - P(Tenpai_opp))`.
    #[must_use]
    pub fn any_opponent_tenpai(&self) -> f64 {
        let mut p_none = 1.0f64;
        for opp in Seat::ALL {
            if opp != self.viewer {
                p_none *= 1.0 - self.signals[opp.index()].p_tenpai.clamp(0.0, 1.0);
            }
        }
        (1.0 - p_none).clamp(0.0, 1.0)
    }

    /// Returns the peak single-opponent weighted danger (`P(Tenpai) * shape_amplification`),
    /// avoiding false-alarm accumulation across three slow early-game opponents.
    #[must_use]
    pub fn peak_weighted_threat(&self) -> f64 {
        let mut peak = 0.0f64;
        for opp in Seat::ALL {
            if opp == self.viewer {
                continue;
            }
            let sig = &self.signals[opp.index()];
            let flush_peak = sig
                .flush_concentration
                .iter()
                .copied()
                .fold(0.0f64, f64::max)
                .max(sig.all_honors_prob);
            let amp = 1.0 + 0.75 * flush_peak + if sig.melds_count >= 2 { 0.25 } else { 0.0 };
            peak = peak.max(sig.p_tenpai * amp);
        }
        peak
    }
}

impl RobKongRiskModel for L3DefenseModel {
    fn rob_kong_risk_ev(&self, view: &PlayerView, tile: Tile) -> f64 {
        // When attempting AddedKong, 3 copies are in our Pon and 1 is in our hand, so
        // `remaining[tile]` is 0. Only sequence waits (for number tiles) or 13o can rob.
        let mut p_none_rob = 1.0f64;
        let mut expected_rob_payout = 0.0f64;
        for opp in Seat::ALL {
            if opp == self.viewer {
                continue;
            }
            let sig = &self.signals[opp.index()];
            let p_wait = opponent_p_wait_given_tenpai(view, opp, tile, &self.remaining, sig);
            if p_wait <= 0.0 {
                continue;
            }
            let flush_suit_conc = number_suit_index(tile)
                .map_or(sig.honor_flush_affinity, |s| sig.flush_concentration[s]);
            let rob_wait = (p_wait * (1.0 + 1.1 * flush_suit_conc)).clamp(0.0, 0.95);
            let p_rob = (sig.p_tenpai * rob_wait).clamp(0.0, 1.0);
            p_none_rob *= 1.0 - p_rob;
            let base_ron = opponent_expected_ron_score(view, opp, tile, &self.remaining, sig);
            let rob_payout = (base_ron * 2.0).min(self.max_payout);
            expected_rob_payout += p_rob * rob_payout;
        }
        let p_any_rob = (1.0 - p_none_rob).clamp(0.0, 1.0);
        // Robbed AddedKong voids the +3 kong delta and forfeits our continuing hand EV (~1.5).
        let raw_ev_loss = expected_rob_payout + p_any_rob * 4.8;
        raw_ev_loss * self.profile.rob_kong_risk_multiplier
    }
}

/// Evaluates all non-fast-path legal candidates with L1-L3 offense-defense synthesis.
#[must_use]
pub fn evaluate_l3_candidates(
    view: &PlayerView,
    ctx: &DecisionContext,
    l4_enabled: bool,
) -> (CandidateBuffer, L3DefenseModel) {
    let profile = L3Profile::from_context(ctx, l4_enabled);
    let model = L3DefenseModel::new(view, profile);
    let mut buf = evaluate_candidates(view, &model);
    let max_opp_tenpai = model.max_opponent_tenpai();
    let any_opp_tenpai = model.any_opponent_tenpai();
    let peak_weighted = model.peak_weighted_threat();
    let threat = threat_level(max_opp_tenpai, any_opp_tenpai, peak_weighted);
    let fold_threat = high_fold_threat(max_opp_tenpai, any_opp_tenpai, peak_weighted);
    let (min_hand_risk, _safe_shield_count) = closed_hand_risk_summary(view, &model);

    for cand in buf.as_mut_slice() {
        let off = style_adjusted_offense(cand, profile);
        match cand.action {
            PlayerAction::Discard { tile, .. } => {
                let eff_w = effective_risk_weight(cand, view, threat, fold_threat, profile);
                cand.ev = off - eff_w * model.discard_risk(tile);
            }
            PlayerAction::Pon { tile } => {
                let shrink = pon_shrink_penalty(cand.shanten_after, cand.ukeire, threat, profile);
                if let Some(updated) =
                    evaluate_pon_with_scorer(view, tile, |pon_cand, disc_tile| {
                        let pon_off = style_adjusted_offense(pon_cand, profile) + profile.meld_bias;
                        let eff_w =
                            effective_risk_weight(pon_cand, view, threat, fold_threat, profile);
                        // Pass still requires discarding on our next turn; charge Pon only for
                        // excess danger beyond the safest tile already in hand.
                        let excess_risk = (model.discard_risk(disc_tile) - min_hand_risk).max(0.0);
                        pon_off - eff_w * excess_risk - shrink
                    })
                {
                    *cand = updated;
                } else {
                    cand.ev = off + profile.meld_bias - shrink;
                }
            }
            PlayerAction::ExposedKong { .. } => {
                let shrink = if cand.shanten_after == 0 {
                    0.04 * profile.meld_shrink_weight * max_opp_tenpai
                } else {
                    profile.meld_shrink_weight
                        * max_opp_tenpai
                        * (0.18 + 0.10 * f64::from(cand.shanten_after.clamp(1, 3)))
                };
                cand.ev = off + profile.meld_bias - shrink;
            }
            PlayerAction::ConcealedKong { .. } => {
                let shrink = if cand.shanten_after >= 1 {
                    0.06 * profile.meld_shrink_weight * max_opp_tenpai
                } else {
                    0.0
                };
                cand.ev = off - shrink;
            }
            PlayerAction::AddedKong { .. } | PlayerAction::Pass => {
                cand.ev = off;
            }
            _ => {
                cand.ev = off;
            }
        }
    }

    (buf, model)
}

/// Runs the L1-L3 decision pipeline with explicit `l4_enabled` control (ready for Ticket #36).
///
/// # Panics
///
/// Panics if `view.legal_actions` is empty.
#[must_use]
pub fn decide_with_options<R: RngCore + ?Sized>(
    view: &PlayerView,
    ctx: &DecisionContext,
    l4_enabled: bool,
    rng: &mut R,
) -> PlayerAction {
    assert!(
        !view.legal_actions.is_empty(),
        "l3::decide requires non-empty legal_actions"
    );

    if let Some(action) = fast_path_action(view) {
        return action;
    }

    let (candidates, _model) = evaluate_l3_candidates(view, ctx, l4_enabled);
    select_best_candidate(candidates.as_slice(), rng).unwrap_or(view.legal_actions[0])
}

/// Runs the default L1-L3 pipeline (`l4_enabled = true` for `Strong`, `false` for `Intermediate`).
///
/// # Panics
///
/// Panics if `view.legal_actions` is empty.
#[must_use]
pub fn decide<R: RngCore + ?Sized>(
    view: &PlayerView,
    ctx: &DecisionContext,
    rng: &mut R,
) -> PlayerAction {
    let l4_enabled = matches!(ctx.difficulty, AiDifficulty::Strong);
    decide_with_options(view, ctx, l4_enabled, rng)
}

fn style_adjusted_offense(cand: &CandidateEv, profile: L3Profile) -> f64 {
    let big_hand_bonus =
        profile.big_hand_bias * cand.p_win * (cand.expected_win_score - 2.0).max(0.0);
    cand.ev + profile.speed_bias * cand.p_win + big_hand_bonus
}

fn closed_hand_risk_summary(view: &PlayerView, model: &L3DefenseModel) -> (f64, usize) {
    let mut best = f64::INFINITY;
    let mut safe_count = 0usize;
    for &t in &view.hand.closed_tiles {
        let r = model.discard_risk(t);
        best = best.min(r);
        if r <= 0.008 {
            safe_count += 1;
        }
    }
    if let Some(t) = view.hand.drawn_tile {
        let r = model.discard_risk(t);
        best = best.min(r);
        if r <= 0.008 {
            safe_count += 1;
        }
    }
    (if best.is_finite() { best } else { 0.0 }, safe_count)
}

fn threat_level(max_opp_tenpai: f64, any_opp_tenpai: f64, peak_weighted: f64) -> f64 {
    let peak_threat = ((max_opp_tenpai - 0.18) / 0.30).clamp(0.0, 1.5);
    let any_threat = ((any_opp_tenpai - 0.42) / 0.38).clamp(0.0, 1.5);
    let weighted_threat = ((peak_weighted - 0.26) / 0.34).clamp(0.0, 1.6);
    peak_threat.max(any_threat).max(weighted_threat)
}

fn high_fold_threat(max_opp_tenpai: f64, any_opp_tenpai: f64, peak_weighted: f64) -> f64 {
    let peak_fold = ((max_opp_tenpai - 0.26) / 0.24).clamp(0.0, 1.9);
    let any_fold = ((any_opp_tenpai - 0.51) / 0.30).clamp(0.0, 1.9);
    let weighted_fold = ((peak_weighted - 0.35) / 0.28).clamp(0.0, 2.1);
    peak_fold.max(any_fold).max(weighted_fold)
}

fn effective_risk_weight(
    cand: &CandidateEv,
    view: &PlayerView,
    threat: f64,
    fold_threat: f64,
    profile: L3Profile,
) -> f64 {
    let late_factor = ((83.0 - f64::from(view.wall_remaining)) / 83.0).clamp(0.0, 1.0);

    let shanten_scale = if cand.shanten_after <= 0 {
        if cand.ukeire == 0 {
            5.00 + 3.50 * fold_threat
        } else {
            0.14 + 0.16 * fold_threat
        }
    } else if cand.shanten_after == 1 {
        if cand.ukeire >= 6 || cand.expected_win_score >= 2.4 {
            let late_guard = if late_factor >= 0.58 {
                1.80 * (late_factor - 0.55) * fold_threat
            } else {
                0.0
            };
            0.10 + 0.25 * threat + late_guard
        } else {
            // Weak 1-shanten (`ukeire <= 5`): fold cleanly under elevated multi-turn threat.
            0.14 + 0.30 * threat + 5.20 * fold_threat + 2.50 * late_factor * fold_threat
        }
    } else {
        // Far from tenpai (`s >= 2`): push cleanly when calm, decisive 7-10x Betaori under tenpai threat.
        0.12 + 0.35 * threat + 7.20 * fold_threat + 2.80 * late_factor * fold_threat
    };

    profile.risk_weight * shanten_scale
}

fn pon_shrink_penalty(shanten_after: i8, ukeire: u16, threat: f64, profile: L3Profile) -> f64 {
    if shanten_after == 0 {
        0.0
    } else if shanten_after == 1 {
        if ukeire >= 6 {
            0.0
        } else {
            0.06 * profile.meld_shrink_weight * (threat - 0.30).max(0.0)
        }
    } else {
        0.14 * profile.meld_shrink_weight * (threat - 0.15).max(0.0)
    }
}

fn number_suit_index(tile: Tile) -> Option<usize> {
    match tile.suit() {
        Suit::Man => Some(0),
        Suit::Bamboo => Some(1),
        Suit::Dot => Some(2),
        Suit::Wind | Suit::Dragon => None,
    }
}

fn is_middle_tile(tile: Tile) -> bool {
    matches!(tile.rank(), Some(3..=7))
}

fn compute_weak_suji(tile: Tile, river: &[mahjong_core::DiscardEntry]) -> f64 {
    let Some(rank) = tile.rank() else {
        return 0.0;
    };
    let base = (tile.index() / 9) * 9;
    let has_rank = |target_rank: u8| -> bool {
        let target_idx = base + usize::from(target_rank - 1);
        river.iter().any(|d| d.tile.index() == target_idx)
    };
    match rank {
        1 => f64::from(u8::from(has_rank(4))),
        2 => f64::from(u8::from(has_rank(5))),
        3 => f64::from(u8::from(has_rank(6))),
        4 => 0.5 * f64::from(u8::from(has_rank(1)) + u8::from(has_rank(7))),
        5 => 0.5 * f64::from(u8::from(has_rank(2)) + u8::from(has_rank(8))),
        6 => 0.5 * f64::from(u8::from(has_rank(3)) + u8::from(has_rank(9))),
        7 => f64::from(u8::from(has_rank(4))),
        8 => f64::from(u8::from(has_rank(5))),
        9 => f64::from(u8::from(has_rank(6))),
        _ => 0.0,
    }
}

fn any_other_orphan_dead(remaining: &[u8; Tile::COUNT], target: Tile) -> bool {
    let target_idx = target.index();
    TERMINAL_OR_HONOR_INDICES
        .iter()
        .any(|&idx| idx != target_idx && remaining[idx] == 0)
}

fn dot<const N: usize>(w: &[f64; N], x: &[f64; N]) -> f64 {
    let mut sum = 0.0f64;
    for i in 0..N {
        sum += w[i] * x[i];
    }
    sum
}

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        let e = (-z).exp();
        1.0 / (1.0 + e)
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}
