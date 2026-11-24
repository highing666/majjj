//! Static logistic-regression weights for the L3 non-Riichi defense model.
//!
//! Calibrated offline via `bench-ai calibrate-l3 --hands 20000 --seed 2026`
//! with zero runtime file I/O.
//!
//! Validation metrics (`80/20` hand-level split):
//! - `tenpai_model`: `train=651039 val=163018 pos_rate=0.1348 pred_mean=0.1340 baseline_log_loss=0.395469 val_log_loss=0.272964 brier=0.085929`
//! - `wait_model`: `train=828249 val=210188 pos_rate=0.0706 pred_mean=0.0705 baseline_log_loss=0.255234 val_log_loss=0.238272 brier=0.063937`

/// Number of features in the opponent `P(Tenpai)` logistic model.
pub const TENPAI_FEATURE_COUNT: usize = 14;

/// Number of features in the conditional `P(Wait(tile) | Tenpai)` logistic model.
pub const WAIT_FEATURE_COUNT: usize = 17;

/// Calibrated weights for `P(Tenpai_opp)` (index 0 = intercept).
pub const TENPAI_WEIGHTS: [f64; TENPAI_FEATURE_COUNT] = [
    -5.18942533, // 0: intercept
    4.39500599,  // 1: wall_progress = (83 - wall_remaining) / 83
    -6.47149002, // 2: wall_progress^2
    0.67076383,  // 3: meld_count >= 1
    0.77118164,  // 4: meld_count >= 2
    0.78444005,  // 5: meld_count >= 3
    0.00000000,  // 6: meld_count >= 4 (handled directly via hand_count <= 1)
    5.15715760,  // 7: river_len / 18
    1.05596081,  // 8: trailing_tsumogiri / 6
    -0.60510461, // 9: wall_progress * (trailing_tsumogiri / 6)
    1.24399971,  // 10: mid_late_middle_tedashi_count / 4
    0.51797226,  // 11: recent_middle_tedashi recency
    2.03682565,  // 12: early_middle_ratio
    -0.51378127, // 13: closed_hand_size / 13
];

/// Calibrated weights for `P(Wait_opp(tile) | Tenpai_opp)` (index 0 = intercept).
pub const WAIT_WEIGHTS: [f64; WAIT_FEATURE_COUNT] = [
    -2.87095004, // 0: intercept
    0.48178236,  // 1: kabe ryanmen combos / 32
    0.16833522,  // 2: kabe kanchan combos / 16
    0.11204788,  // 3: kabe penchan combos / 16
    -0.24203952, // 4: kabe shanpon combos / 6
    0.76831505,  // 5: kabe tanki combos / 4
    -2.05322371, // 6: zone_honor
    -0.48868766, // 7: zone_terminal (1, 9)
    -0.14665494, // 8: zone_2_8
    0.12639514,  // 9: zone_3_7
    -3.70677691, // 10: recent_genbutsu (opp river last 4, non-Riichi downweighted)
    -2.45421363, // 11: early_genbutsu (opp river earlier, heavily downweighted)
    -0.57003041, // 12: weak_suji (non-Riichi weak prior)
    0.33766868,  // 13: flush_suit_match (tile matches opp flush/honor concentration)
    -0.29079674, // 14: off_flush_suit (tile is number tile in non-flush suit)
    0.69119330,  // 15: multi_pon_shanpon_tanki boost
    -0.51404850, // 16: zero_std_waits fallback (when only 13o theoretically open)
];
