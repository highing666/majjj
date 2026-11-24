//! Pure in-memory 4-seat duplicate evaluation CLI (`bench-ai`).

use std::env;
use std::process::ExitCode;
use std::str::FromStr;

use mahjong_ai::eval::{
    default_eval_threads, run_duplicate_matchup, EvalConfig, EvalReport, EvalStrategy,
};
use mahjong_ai::DecisionBudget;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_and_run(&args) {
        Ok(Some(report)) => {
            print_report(&report);
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

fn parse_and_run(args: &[String]) -> Result<Option<EvalReport>, String> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        return Ok(None);
    }

    match args[0].as_str() {
        "matchup" => {
            let cfg = parse_matchup_args(&args[1..])?;
            let report = run_duplicate_matchup(&cfg).map_err(|e| e.to_string())?;
            Ok(Some(report))
        }
        other => Err(format!(
            "unknown subcommand '{other}' (expected: matchup; pass --help for usage)"
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

fn print_usage() {
    println!(
        "Usage:
  bench-ai matchup [--candidate <name>] [--opponent <name>] [--groups <N>] [--seed <u64>] [--threads <T>] [--budget deterministic:<S>|realtime:<S>:<soft>:<hard>]

Strategies:
  random | tile-efficiency | intermediate | strong-no-l4 | strong"
    );
}
