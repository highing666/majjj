mod common;

use common::tiles;
use mahjong_ai::eval::{default_eval_threads, run_duplicate_matchup, EvalConfig, EvalStrategy};
use mahjong_ai::{DecisionBudget, RandomBot};
use mahjong_core::{
    build_preset_wall, project_view, Match, MatchMode, PlayerAction, PresetWallProvider,
    PublicPhase, Seat, SeededWallProvider, TuidaohuRuleset,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

#[test]
#[should_panic(expected = "legal_actions")]
fn random_bot_panics_when_legal_actions_is_empty() {
    let wall = build_preset_wall(
        &tiles("123m 456m 789m 111p 22p"),
        &tiles("111s 222s 333s 444s E"),
        &tiles("555s 666s 777s 888s S"),
        &tiles("999s EEE SSS WWW N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");
    let south_view = project_view(&game, Seat::South);
    assert!(south_view.legal_actions.is_empty());

    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let _ = RandomBot::decide(&south_view, &mut rng);
}

#[test]
fn random_bot_confirms_next_hand_in_between_hands_and_plays_uniformly_without_panics() {
    let wall = build_preset_wall(
        &tiles("123m 456m 789m 111p 22p"),
        &tiles("111s 222s 333s 444s E"),
        &tiles("555s 666s 777s 888s S"),
        &tiles("999s EEE SSS WWW N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");

    // During Playing, RandomBot explores multiple legal actions across seeds (including Tsumo and discards).
    let east_view = project_view(&game, Seat::East);
    assert!(east_view.legal_actions.len() > 2);
    let mut seen_distinct = std::collections::HashSet::new();
    for seed in 0..128u64 {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let action = RandomBot::decide(&east_view, &mut rng);
        assert!(east_view.legal_actions.contains(&action));
        seen_distinct.insert(action);
    }
    assert!(
        seen_distinct.len() >= 3,
        "expected uniform sampling across legal actions, only saw {seen_distinct:?}"
    );

    // In BetweenHands (FreeTable), RandomBot always picks ConfirmNextHand and never EndMatch.
    game.apply_action(Seat::East, PlayerAction::Tsumo)
        .expect("tsumo settles hand");
    let between_view = project_view(&game, Seat::East);
    assert_eq!(between_view.phase, PublicPhase::BetweenHands);
    assert!(between_view
        .legal_actions
        .contains(&PlayerAction::ConfirmNextHand));
    assert!(between_view.legal_actions.contains(&PlayerAction::EndMatch));
    for seed in 0..32u64 {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        assert_eq!(
            RandomBot::decide(&between_view, &mut rng),
            PlayerAction::ConfirmNextHand
        );
    }

    // Play 8 complete EastOnly matches with 4 RandomBots, checking strict legality and zero deadlocks.
    for seed in 1..=8u64 {
        let mut m = Match::new(
            MatchMode::EastOnly,
            TuidaohuRuleset,
            SeededWallProvider::new(seed),
        )
        .expect("match initializes");
        let mut rngs: [ChaCha8Rng; 4] =
            std::array::from_fn(|i| ChaCha8Rng::seed_from_u64(seed * 97 + i as u64));

        loop {
            let views = Seat::ALL.map(|s| project_view(&m, s));
            if views[0].phase == PublicPhase::MatchFinished {
                break;
            }
            let mut stepped = false;
            for (idx, view) in views.iter().enumerate() {
                if view.legal_actions.is_empty() {
                    continue;
                }
                if view.phase == PublicPhase::BetweenHands
                    && !view.legal_actions.contains(&PlayerAction::ConfirmNextHand)
                {
                    continue;
                }
                let action = RandomBot::decide(view, &mut rngs[idx]);
                assert!(view.legal_actions.contains(&action));
                if view.phase == PublicPhase::BetweenHands {
                    assert_eq!(action, PlayerAction::ConfirmNextHand);
                }
                m.apply_action(view.viewer, action)
                    .expect("legal RandomBot action applies cleanly");
                stepped = true;
                break;
            }
            assert!(stepped, "deadlock in RandomBot match");
        }
    }
}

#[test]
fn duplicate_eval_tile_efficiency_beats_random_and_is_thread_count_deterministic() {
    let default_threads = default_eval_threads();
    assert!((1..=4).contains(&default_threads));

    let cfg_t1 = EvalConfig {
        candidate: EvalStrategy::TileEfficiency,
        opponent: EvalStrategy::Random,
        groups: 25,
        seed: 42,
        threads: 1,
        budget: DecisionBudget::Deterministic { max_samples: 64 },
    };
    let cfg_t4 = EvalConfig {
        threads: 4,
        ..cfg_t1
    };

    let rep_t1 = run_duplicate_matchup(&cfg_t1).expect("eval t1 succeeds");
    let rep_t4 = run_duplicate_matchup(&cfg_t4).expect("eval t4 succeeds");

    assert_eq!(rep_t1.groups, 25);
    assert_eq!(rep_t1.total_hands, 100);
    assert_eq!(
        rep_t1.mean_net_score.to_bits(),
        rep_t4.mean_net_score.to_bits()
    );
    assert_eq!(rep_t1.std_err.to_bits(), rep_t4.std_err.to_bits());
    assert_eq!(rep_t1.ci95_low.to_bits(), rep_t4.ci95_low.to_bits());
    assert_eq!(rep_t1.ci95_high.to_bits(), rep_t4.ci95_high.to_bits());
    assert_eq!(rep_t1.tsumo_count, rep_t4.tsumo_count);
    assert_eq!(rep_t1.ron_count, rep_t4.ron_count);
    assert_eq!(rep_t1.deal_in_count, rep_t4.deal_in_count);
    assert_eq!(rep_t1.draw_game_count, rep_t4.draw_game_count);
    assert_eq!(rep_t1.latency.samples, rep_t4.latency.samples);

    assert!(
        rep_t1.mean_net_score > 0.5 && rep_t1.ci95_low > 0.0,
        "expected TileEfficiency to significantly beat Random: {rep_t1:?}"
    );
    assert!(rep_t1.tsumo_rate + rep_t1.ron_rate > 0.1);
    assert!(rep_t1.latency.samples > 0);
    assert!(rep_t1.latency.p50_ms <= rep_t1.latency.p95_ms);
    assert!(rep_t1.latency.p95_ms <= rep_t1.latency.p99_ms);
    assert!(rep_t1.latency.p99_ms <= rep_t1.latency.max_ms);

    let bad_cfg = EvalConfig {
        threads: 0,
        ..cfg_t1
    };
    assert!(run_duplicate_matchup(&bad_cfg).is_err());
}

#[test]
fn bench_ai_cli_runs_matchup_and_rejects_invalid_threads() {
    let bin = env!("CARGO_BIN_EXE_bench-ai");
    let ok_out = std::process::Command::new(bin)
        .args([
            "matchup",
            "--candidate",
            "tile-efficiency",
            "--opponent",
            "random",
            "--groups",
            "10",
            "--seed",
            "1",
            "--threads",
            "2",
            "--budget",
            "deterministic:64",
        ])
        .output()
        .expect("spawn bench-ai");
    assert!(
        ok_out.status.success(),
        "bench-ai failed: stderr={}",
        String::from_utf8_lossy(&ok_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&ok_out.stdout);
    assert!(
        stdout.contains("candidate: tile-efficiency"),
        "stdout={stdout}"
    );
    assert!(stdout.contains("opponent: random"), "stdout={stdout}");
    assert!(stdout.contains("mean_net_score:"), "stdout={stdout}");
    assert!(stdout.contains("ci95:"), "stdout={stdout}");
    assert!(stdout.contains("tsumo_rate:"), "stdout={stdout}");
    assert!(stdout.contains("ron_rate:"), "stdout={stdout}");
    assert!(stdout.contains("deal_in_rate:"), "stdout={stdout}");
    assert!(stdout.contains("draw_game_rate:"), "stdout={stdout}");
    assert!(stdout.contains("latency_ms:"), "stdout={stdout}");
    assert!(stdout.contains("p95="), "stdout={stdout}");

    let bad_out = std::process::Command::new(bin)
        .args([
            "matchup",
            "--candidate",
            "tile-efficiency",
            "--opponent",
            "random",
            "--groups",
            "5",
            "--threads",
            "0",
        ])
        .output()
        .expect("spawn bench-ai with invalid threads");
    assert!(
        !bad_out.status.success(),
        "expected non-zero exit for --threads 0"
    );

    let rt_out = std::process::Command::new(bin)
        .args([
            "matchup",
            "--candidate",
            "strong-no-l4",
            "--opponent",
            "intermediate",
            "--groups",
            "2",
            "--seed",
            "9",
            "--threads",
            "1",
            "--budget",
            "realtime:32:120:180",
        ])
        .output()
        .expect("spawn bench-ai with realtime budget");
    assert!(
        rt_out.status.success(),
        "bench-ai realtime budget failed: stderr={}",
        String::from_utf8_lossy(&rt_out.stderr)
    );
    let rt_stdout = String::from_utf8_lossy(&rt_out.stdout);
    assert!(
        rt_stdout.contains("candidate: strong-no-l4"),
        "rt_stdout={rt_stdout}"
    );
    assert!(
        rt_stdout.contains("budget: realtime:32:120:180"),
        "rt_stdout={rt_stdout}"
    );
}

#[test]
fn bench_ai_calibrate_l3_runs_in_memory_and_emits_metrics_and_const_tables() {
    use mahjong_ai::eval::{run_calibrate_l3, CalibrateConfig};

    let cfg = CalibrateConfig {
        hands: 40,
        seed: 2026,
        threads: 2,
    };
    let report = run_calibrate_l3(&cfg).expect("calibration succeeds");
    assert_eq!(report.hands, 40);
    assert!(report.tenpai_metrics.val_samples > 0);
    assert!(report.wait_metrics.val_samples > 0);
    assert!(
        report.tenpai_metrics.val_log_loss < report.tenpai_metrics.val_baseline_log_loss,
        "tenpai model must improve validation log-loss over prior baseline: {:?}",
        report.tenpai_metrics
    );
    assert!(
        report.wait_metrics.val_log_loss <= report.wait_metrics.val_baseline_log_loss + 0.02,
        "wait model validation log-loss should be bounded near or below baseline on small sample: {:?}",
        report.wait_metrics
    );
    assert!(report.tenpai_weights.iter().all(|w| w.is_finite()));
    assert!(report.wait_weights.iter().all(|w| w.is_finite()));

    let bad_hands = CalibrateConfig { hands: 0, ..cfg };
    assert!(run_calibrate_l3(&bad_hands).is_err());
    let bad_threads = CalibrateConfig { threads: 0, ..cfg };
    assert!(run_calibrate_l3(&bad_threads).is_err());

    let bin = env!("CARGO_BIN_EXE_bench-ai");
    let cli_out = std::process::Command::new(bin)
        .args([
            "calibrate-l3",
            "--hands",
            "30",
            "--seed",
            "2026",
            "--threads",
            "2",
        ])
        .output()
        .expect("spawn bench-ai calibrate-l3");
    assert!(
        cli_out.status.success(),
        "bench-ai calibrate-l3 failed: stderr={}",
        String::from_utf8_lossy(&cli_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&cli_out.stdout);
    assert!(stdout.contains("TENPAI_WEIGHTS"), "stdout={stdout}");
    assert!(stdout.contains("WAIT_WEIGHTS"), "stdout={stdout}");
    assert!(stdout.contains("val_log_loss="), "stdout={stdout}");
    assert!(
        stdout.contains("pub const TENPAI_WEIGHTS"),
        "stdout={stdout}"
    );
    assert!(stdout.contains("pub const WAIT_WEIGHTS"), "stdout={stdout}");
}

#[test]
fn smoke_and_gate_apis_and_cli_enforce_slice3_thresholds() {
    use mahjong_ai::eval::{
        run_gate, run_smoke_gate, verify_cross_thread_determinism, GateConfig, SmokeConfig,
    };

    let smoke_def = SmokeConfig::default();
    assert_eq!(smoke_def.groups, 100);
    assert_eq!(smoke_def.max_samples, 32);
    assert_eq!(smoke_def.threads, default_eval_threads());

    let gate_def = GateConfig::default();
    assert_eq!(gate_def.groups, 1000);
    assert_eq!(gate_def.groups * 4, 4000);
    assert_eq!(gate_def.det_samples, 128);
    assert!(gate_def.realtime_groups * 4 >= 60);
    assert_eq!(gate_def.threads, default_eval_threads());

    let det = verify_cross_thread_determinism(1, 2026, 4).expect("determinism check runs");
    assert!(det.traces_match);
    assert!(det.scores_bit_exact);
    assert!(det.passed);

    let small_smoke = run_smoke_gate(&SmokeConfig {
        groups: 1,
        seed: 2026,
        threads: default_eval_threads(),
        max_samples: 4,
    })
    .expect("small smoke succeeds");
    assert_eq!(small_smoke.report.total_hands, 4);
    assert_eq!(small_smoke.passed, small_smoke.report.mean_net_score > 0.0);

    let small_gate = run_gate(&GateConfig {
        groups: 1,
        det_samples: 4,
        realtime_groups: 1,
        determinism_groups: 1,
        seed: 2026,
        threads: default_eval_threads(),
    })
    .expect("small gate runs");
    assert_eq!(small_gate.strong.total_hands, 4);
    assert_eq!(small_gate.realtime.total_hands, 4);
    assert!(small_gate.determinism.passed);
    let expected_reduction = if small_gate.tile_efficiency.deal_in_rate > 0.0 {
        (small_gate.tile_efficiency.deal_in_rate - small_gate.strong.deal_in_rate)
            / small_gate.tile_efficiency.deal_in_rate
    } else {
        0.0
    };
    assert!((small_gate.deal_in_reduction - expected_reduction).abs() < 1e-12);

    let bin = env!("CARGO_BIN_EXE_bench-ai");
    let smoke_cli = std::process::Command::new(bin)
        .args(["smoke", "--groups", "1", "--seed", "2026", "--samples", "4"])
        .output()
        .expect("spawn bench-ai smoke");
    let smoke_stdout = String::from_utf8_lossy(&smoke_cli.stdout);
    assert!(
        smoke_stdout.contains("bench-ai smoke"),
        "stdout={smoke_stdout}"
    );
    assert!(
        smoke_stdout.contains("mean_net_score"),
        "stdout={smoke_stdout}"
    );

    let gate_cli = std::process::Command::new(bin)
        .args([
            "gate",
            "--groups",
            "1",
            "--det-samples",
            "4",
            "--realtime-groups",
            "1",
            "--determinism-groups",
            "1",
            "--seed",
            "2026",
        ])
        .output()
        .expect("spawn bench-ai gate");
    let gate_stdout = String::from_utf8_lossy(&gate_cli.stdout);
    assert!(
        gate_stdout.contains("5-Level Ablation"),
        "stdout={gate_stdout}"
    );
    assert!(
        gate_stdout.contains("4,000-Hand Hard Gate") || gate_stdout.contains("Hard Threshold"),
        "stdout={gate_stdout}"
    );
    assert!(gate_stdout.contains("Determinism"), "stdout={gate_stdout}");
    assert!(gate_stdout.contains("Realtime"), "stdout={gate_stdout}");
    // On 3 groups with current ruleset, mean_net_score < 1.5 so gate MUST exit non-zero.
    assert!(
        !gate_cli.status.success(),
        "expected gate failure to exit non-zero"
    );
}
