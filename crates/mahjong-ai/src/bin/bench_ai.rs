//! Pure in-memory 4-seat duplicate evaluation CLI (`bench-ai`).

use std::env;
use std::process::ExitCode;
use std::str::FromStr;

use mahjong_ai::eval::{
    default_eval_threads, run_calibrate_l3, run_duplicate_matchup, run_gate, run_smoke_gate,
    CalibrateConfig, CalibrateReport, EvalConfig, EvalReport, EvalStrategy, GateConfig, GateReport,
    LogisticMetrics, SmokeConfig, SmokeReport, GATE_MAX_REALTIME_MAX_MS, GATE_MAX_REALTIME_P95_MS,
    GATE_MIN_DEAL_IN_REDUCTION, GATE_MIN_MEAN_NET_SCORE,
};
use mahjong_ai::l3_weights::{TENPAI_FEATURE_COUNT, WAIT_FEATURE_COUNT};
use mahjong_ai::DecisionBudget;

enum CliOutput {
    Matchup(EvalReport),
    Smoke(SmokeReport),
    Gate(Box<GateReport>),
    Calibrate(CalibrateReport),
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_and_run(&args) {
        Ok(Some(CliOutput::Matchup(report))) => {
            print_report(&report);
            ExitCode::SUCCESS
        }
        Ok(Some(CliOutput::Smoke(report))) => {
            print_smoke(&report);
            if report.passed {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "smoke gate FAILED: mean_net_score ({:+.4}) <= 0.0",
                    report.report.mean_net_score
                );
                ExitCode::FAILURE
            }
        }
        Ok(Some(CliOutput::Gate(report))) => {
            print_gate(&report);
            if report.passed {
                ExitCode::SUCCESS
            } else {
                eprintln!("bench-ai gate FAILED (one or more Slice 3 hard thresholds unmet)");
                ExitCode::FAILURE
            }
        }
        Ok(Some(CliOutput::Calibrate(report))) => {
            print_calibration(&report);
            ExitCode::SUCCESS
        }
        Ok(None) => {
            print_usage();
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn parse_and_run(args: &[String]) -> Result<Option<CliOutput>, String> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        return Ok(None);
    }

    match args[0].as_str() {
        "matchup" => {
            let cfg = parse_matchup_args(&args[1..])?;
            let report = run_duplicate_matchup(&cfg).map_err(|e| e.to_string())?;
            Ok(Some(CliOutput::Matchup(report)))
        }
        "smoke" => {
            let cfg = parse_smoke_args(&args[1..])?;
            let report = run_smoke_gate(&cfg).map_err(|e| e.to_string())?;
            Ok(Some(CliOutput::Smoke(report)))
        }
        "gate" => {
            let cfg = parse_gate_args(&args[1..])?;
            let report = run_gate(&cfg).map_err(|e| e.to_string())?;
            Ok(Some(CliOutput::Gate(Box::new(report))))
        }
        "calibrate-l3" => {
            let cfg = parse_calibrate_args(&args[1..])?;
            let report = run_calibrate_l3(&cfg).map_err(|e| e.to_string())?;
            Ok(Some(CliOutput::Calibrate(report)))
        }
        other => Err(format!(
            "unknown subcommand '{other}' (expected: matchup, smoke, gate, calibrate-l3; pass --help for usage)"
        )),
    }
}

fn parse_matchup_args(args: &[String]) -> Result<EvalConfig, String> {
    let mut candidate = EvalStrategy::TileEfficiency;
    let mut opponent = EvalStrategy::Random;
    let mut groups = 100usize;
    let mut seed = 1u64;
    let mut threads = default_eval_threads();
    let mut budget = DecisionBudget::Deterministic { max_samples: 128 };

    let mut idx = 0usize;
    while idx < args.len() {
        let arg = args[idx].as_str();
        if matches!(arg, "-h" | "--help") {
            return Err(
                "usage: bench-ai matchup --candidate <name> --opponent <name> --groups <N> --seed <u64> --threads <T> [--budget deterministic:<S>|realtime:<S>:<soft>:<hard>]"
                    .to_string(),
            );
        }

        let (flag, value) = if let Some((k, v)) = arg.split_once('=') {
            idx += 1;
            (k, v)
        } else {
            let val = args
                .get(idx + 1)
                .ok_or_else(|| format!("missing value for flag '{arg}'"))?;
            idx += 2;
            (arg, val.as_str())
        };

        match flag {
            "--candidate" => {
                candidate = EvalStrategy::from_str(value).map_err(|e| e.to_string())?;
            }
            "--opponent" => {
                opponent = EvalStrategy::from_str(value).map_err(|e| e.to_string())?;
            }
            "--groups" => {
                groups = value
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --groups '{value}': {e}"))?;
            }
            "--seed" => {
                seed = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --seed '{value}': {e}"))?;
            }
            "--threads" => {
                threads = value
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --threads '{value}': {e}"))?;
            }
            "--budget" => {
                budget = parse_budget(value)?;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }

    Ok(EvalConfig {
        candidate,
        opponent,
        groups,
        seed,
        threads,
        budget,
    })
}

fn parse_calibrate_args(args: &[String]) -> Result<CalibrateConfig, String> {
    let mut hands = 20_000usize;
    let mut seed = 2026u64;
    let mut threads = default_eval_threads();

    let mut idx = 0usize;
    while idx < args.len() {
        let arg = args[idx].as_str();
        if matches!(arg, "-h" | "--help") {
            return Err(
                "usage: bench-ai calibrate-l3 --hands <N> --seed <u64> --threads <T>".to_string(),
            );
        }

        let (flag, value) = if let Some((k, v)) = arg.split_once('=') {
            idx += 1;
            (k, v)
        } else {
            let val = args
                .get(idx + 1)
                .ok_or_else(|| format!("missing value for flag '{arg}'"))?;
            idx += 2;
            (arg, val.as_str())
        };

        match flag {
            "--hands" => {
                hands = value
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --hands '{value}': {e}"))?;
            }
            "--seed" => {
                seed = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --seed '{value}': {e}"))?;
            }
            "--threads" => {
                threads = value
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --threads '{value}': {e}"))?;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }

    Ok(CalibrateConfig {
        hands,
        seed,
        threads,
    })
}

fn next_flag_value<'a>(args: &'a [String], idx: &mut usize) -> Result<(&'a str, &'a str), String> {
    let arg = args[*idx].as_str();
    if let Some((k, v)) = arg.split_once('=') {
        *idx += 1;
        Ok((k, v))
    } else {
        let val = args
            .get(*idx + 1)
            .ok_or_else(|| format!("missing value for flag '{arg}'"))?;
        *idx += 2;
        Ok((arg, val.as_str()))
    }
}

fn parse_usize_flag(flag: &str, value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|e| format!("invalid {flag} '{value}': {e}"))
}

fn parse_smoke_args(args: &[String]) -> Result<SmokeConfig, String> {
    let mut cfg = SmokeConfig::default();
    let mut idx = 0usize;
    while idx < args.len() {
        let arg = args[idx].as_str();
        if matches!(arg, "-h" | "--help") {
            return Err(
                "usage: bench-ai smoke [--groups <N>] [--seed <u64>] [--threads <T>] [--samples <S>]"
                    .to_string(),
            );
        }
        let (flag, value) = next_flag_value(args, &mut idx)?;
        match flag {
            "--groups" => cfg.groups = parse_usize_flag(flag, value)?,
            "--seed" => {
                cfg.seed = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --seed '{value}': {e}"))?;
            }
            "--threads" => cfg.threads = parse_usize_flag(flag, value)?,
            "--samples" => {
                let s = parse_usize_flag(flag, value)?;
                if s == 0 {
                    return Err("--samples must be >= 1".to_string());
                }
                cfg.max_samples = s;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(cfg)
}

fn parse_gate_args(args: &[String]) -> Result<GateConfig, String> {
    let mut cfg = GateConfig::default();
    let mut idx = 0usize;
    while idx < args.len() {
        let arg = args[idx].as_str();
        if matches!(arg, "-h" | "--help") {
            return Err(
                "usage: bench-ai gate [--groups <N>] [--det-samples <S>] [--realtime-groups <N>] [--determinism-groups <N>] [--seed <u64>] [--threads <T>]"
                    .to_string(),
            );
        }
        let (flag, value) = next_flag_value(args, &mut idx)?;
        match flag {
            "--groups" => cfg.groups = parse_usize_flag(flag, value)?,
            "--det-samples" => {
                let s = parse_usize_flag(flag, value)?;
                if s == 0 {
                    return Err("--det-samples must be >= 1".to_string());
                }
                cfg.det_samples = s;
            }
            "--realtime-groups" => cfg.realtime_groups = parse_usize_flag(flag, value)?,
            "--determinism-groups" => cfg.determinism_groups = parse_usize_flag(flag, value)?,
            "--seed" => {
                cfg.seed = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --seed '{value}': {e}"))?;
            }
            "--threads" => cfg.threads = parse_usize_flag(flag, value)?,
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(cfg)
}

fn parse_budget(spec: &str) -> Result<DecisionBudget, String> {
    let parts: Vec<&str> = spec.split(':').collect();
    match parts.as_slice() {
        ["deterministic", samples] => {
            let max_samples = samples
                .parse::<usize>()
                .map_err(|e| format!("invalid deterministic sample count '{samples}': {e}"))?;
            if max_samples == 0 {
                return Err("deterministic max_samples must be >= 1".to_string());
            }
            Ok(DecisionBudget::Deterministic { max_samples })
        }
        ["realtime", samples, soft, hard] => {
            let max_samples = samples
                .parse::<usize>()
                .map_err(|e| format!("invalid realtime sample count '{samples}': {e}"))?;
            let soft_ms = soft
                .parse::<u64>()
                .map_err(|e| format!("invalid soft_ms '{soft}': {e}"))?;
            let hard_ms = hard
                .parse::<u64>()
                .map_err(|e| format!("invalid hard_ms '{hard}': {e}"))?;
            if max_samples == 0 {
                return Err("realtime max_samples must be >= 1".to_string());
            }
            if soft_ms > hard_ms {
                return Err(format!(
                    "realtime soft_ms ({soft_ms}) must be <= hard_ms ({hard_ms})"
                ));
            }
            Ok(DecisionBudget::Realtime {
                max_samples,
                soft_ms,
                hard_ms,
            })
        }
        _ => Err(format!(
            "invalid --budget '{spec}' (expected deterministic:<S> or realtime:<S>:<soft>:<hard>)"
        )),
    }
}

fn format_budget(budget: DecisionBudget) -> String {
    match budget {
        DecisionBudget::Deterministic { max_samples } => format!("deterministic:{max_samples}"),
        DecisionBudget::Realtime {
            max_samples,
            soft_ms,
            hard_ms,
        } => format!("realtime:{max_samples}:{soft_ms}:{hard_ms}"),
    }
}

fn print_report(report: &EvalReport) {
    println!("=== bench-ai duplicate matchup ===");
    println!("candidate: {}", report.candidate);
    println!("opponent: {}", report.opponent);
    println!("groups: {} (hands: {})", report.groups, report.total_hands);
    println!(
        "seed: {} | threads: {} | budget: {}",
        report.seed,
        report.threads,
        format_budget(report.budget)
    );
    println!("mean_net_score: {:+.4} pts/hand", report.mean_net_score);
    println!(
        "ci95: [{:+.4}, {:+.4}] (se={:.4})",
        report.ci95_low, report.ci95_high, report.std_err
    );
    println!(
        "tsumo_rate: {:.2}% ({}/{})",
        report.tsumo_rate * 100.0,
        report.tsumo_count,
        report.total_hands
    );
    println!(
        "ron_rate: {:.2}% ({}/{})",
        report.ron_rate * 100.0,
        report.ron_count,
        report.total_hands
    );
    println!(
        "deal_in_rate: {:.2}% ({}/{})",
        report.deal_in_rate * 100.0,
        report.deal_in_count,
        report.total_hands
    );
    println!(
        "draw_game_rate: {:.2}% ({}/{})",
        report.draw_game_rate * 100.0,
        report.draw_game_count,
        report.total_hands
    );
    println!(
        "latency_ms: samples={} mean={:.4} p50={:.4} p95={:.4} p99={:.4} max={:.4}",
        report.latency.samples,
        report.latency.mean_ms,
        report.latency.p50_ms,
        report.latency.p95_ms,
        report.latency.p99_ms,
        report.latency.max_ms
    );
}

fn print_smoke(smoke: &SmokeReport) {
    println!("=== bench-ai smoke ===");
    print_report(&smoke.report);
    println!(
        "smoke_gate: {} (mean_net_score={:+.4} > 0.0)",
        if smoke.passed { "PASS" } else { "FAIL" },
        smoke.report.mean_net_score
    );
}

fn print_gate(gate: &GateReport) {
    println!("=== bench-ai gate (Slice 3 Hard Gates) ===");
    println!(
        "groups={} ({} hands) | det_samples={} | realtime_groups={} ({} hands) | determinism_groups={} | seed={} | threads={}",
        gate.config.groups,
        gate.strong.total_hands,
        gate.config.det_samples,
        gate.config.realtime_groups,
        gate.realtime.total_hands,
        gate.config.determinism_groups,
        gate.config.seed,
        gate.config.threads
    );
    println!();
    println!("[1] 5-Level Ablation Monotonicity (vs 3x tile-efficiency):");
    println!(
        "{:<18} | {:>10} | {:>19} | {:>7} | {:>7} | {:>8}",
        "Strategy", "Mean Net", "95% CI", "Tsumo%", "Ron%", "DealIn%"
    );
    println!("{}", "-".repeat(82));
    for r in [
        &gate.random,
        &gate.tile_efficiency,
        &gate.intermediate,
        &gate.strong_no_l4,
        &gate.strong,
    ] {
        println!(
            "{:<18} | {:>+10.4} | [{:>+7.4}, {:>+7.4}] | {:>6.2}% | {:>6.2}% | {:>7.2}%",
            r.candidate.as_str(),
            r.mean_net_score,
            r.ci95_low,
            r.ci95_high,
            r.tsumo_rate * 100.0,
            r.ron_rate * 100.0,
            r.deal_in_rate * 100.0,
        );
    }
    println!(
        "Gate 1 (random < tile-efficiency < intermediate < strong-no-l4 <= strong): {}",
        pass_fail(gate.ablation_monotonic)
    );
    println!();
    println!("[2] 4,000-Hand Hard Threshold (Strong vs 3x TileEfficiencyBot):");
    println!(
        "  mean_net_score = {:+.4} pts/hand (req >= +{:.2}): {}",
        gate.strong.mean_net_score,
        GATE_MIN_MEAN_NET_SCORE,
        pass_fail(gate.threshold_mean_passed)
    );
    println!(
        "  ci95_low       = {:+.4} pts/hand (req > 0.00): {}",
        gate.strong.ci95_low,
        pass_fail(gate.threshold_ci_passed)
    );
    println!(
        "  deal_in_reduct = {:.2}% (baseline={:.2}%, strong={:.2}%, req >= {:.0}%): {}",
        gate.deal_in_reduction * 100.0,
        gate.tile_efficiency.deal_in_rate * 100.0,
        gate.strong.deal_in_rate * 100.0,
        GATE_MIN_DEAL_IN_REDUCTION * 100.0,
        pass_fail(gate.threshold_deal_in_passed)
    );
    println!("Gate 2 Overall: {}", pass_fail(gate.threshold_passed));
    println!();
    println!("[3] Cross-Thread Bit-Exact Determinism (1 vs 4 Rayon threads):");
    println!(
        "  groups={} actions={} traces_match={} scores_bit_exact={} (mean={:+.4}): {}",
        gate.determinism.groups,
        gate.determinism.action_count,
        gate.determinism.traces_match,
        gate.determinism.scores_bit_exact,
        gate.determinism.t1_report.mean_net_score,
        pass_fail(gate.determinism_passed)
    );
    println!();
    println!(
        "[4] Realtime Latency Distribution (threads={}):",
        gate.realtime.threads
    );
    println!(
        "  hands={} samples={} mean={:.4} ms p50={:.4} ms p95={:.4} ms (req <= {:.1}) p99={:.4} ms max={:.4} ms (req <= {:.1}): {}",
        gate.realtime.total_hands,
        gate.realtime.latency.samples,
        gate.realtime.latency.mean_ms,
        gate.realtime.latency.p50_ms,
        gate.realtime.latency.p95_ms,
        GATE_MAX_REALTIME_P95_MS,
        gate.realtime.latency.p99_ms,
        gate.realtime.latency.max_ms,
        GATE_MAX_REALTIME_MAX_MS,
        pass_fail(gate.realtime_passed)
    );
    println!();
    println!("ALL SLICE 3 GATES: {}", pass_fail(gate.passed));
}

const fn pass_fail(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

fn print_calibration(report: &CalibrateReport) {
    println!("=== bench-ai calibrate-l3 ===");
    println!(
        "hands: {} | seed: {} | threads: {}",
        report.hands, report.seed, report.threads
    );
    print_metrics("tenpai_model", &report.tenpai_metrics);
    print_metrics("wait_model", &report.wait_metrics);
    println!();
    println!("/// Calibrated weights for `P(Tenpai_opp)` (index 0 = intercept).");
    println!("pub const TENPAI_WEIGHTS: [f64; TENPAI_FEATURE_COUNT] = [");
    let _ = TENPAI_FEATURE_COUNT;
    for (idx, w) in report.tenpai_weights.iter().enumerate() {
        println!("    {w:.8}, // {idx}");
    }
    println!("];");
    println!();
    println!("/// Calibrated weights for `P(Wait_opp(tile) | Tenpai_opp)` (index 0 = intercept).");
    println!("pub const WAIT_WEIGHTS: [f64; WAIT_FEATURE_COUNT] = [");
    let _ = WAIT_FEATURE_COUNT;
    for (idx, w) in report.wait_weights.iter().enumerate() {
        println!("    {w:.8}, // {idx}");
    }
    println!("];");
}

fn print_metrics(label: &str, m: &LogisticMetrics) {
    println!(
        "{label}: train={} val={} pos_rate={:.4} pred_mean={:.4} baseline_log_loss={:.6} val_log_loss={:.6} brier={:.6}",
        m.train_samples,
        m.val_samples,
        m.val_pos_rate,
        m.val_pred_mean,
        m.val_baseline_log_loss,
        m.val_log_loss,
        m.val_brier
    );
}

fn print_usage() {
    println!(
        "Usage:
  bench-ai matchup [--candidate <name>] [--opponent <name>] [--groups <N>] [--seed <u64>] [--threads <T>] [--budget deterministic:<S>|realtime:<S>:<soft>:<hard>]
  bench-ai smoke [--groups <N>] [--seed <u64>] [--threads <T>] [--samples <S>]
  bench-ai gate [--groups <N>] [--det-samples <S>] [--realtime-groups <N>] [--determinism-groups <N>] [--seed <u64>] [--threads <T>]
  bench-ai calibrate-l3 [--hands <N>] [--seed <u64>] [--threads <T>]

Strategies:
  random | tile-efficiency | intermediate | strong-no-l4 | strong"
    );
}
