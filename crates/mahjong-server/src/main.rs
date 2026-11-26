//! CLI entry point for `mahjong-server`.

use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use mahjong_server::{default_ai_threads, ServerConfig};

fn parse_ai_threads(raw: &str) -> Result<usize, String> {
    let parsed: usize = raw
        .parse()
        .map_err(|err| format!("invalid ai thread count '{raw}': {err}"))?;
    if parsed == 0 {
        return Err("ai thread count must be at least 1".to_string());
    }
    Ok(parsed)
}

/// Authoritative LAN server for Majjj.
#[derive(Debug, Parser)]
#[command(
    name = "mahjong-server",
    version,
    about = "Majjj LAN mahjong table server"
)]
struct Cli {
    /// TCP port to listen on.
    #[arg(long, env = "MAJJJ_PORT", default_value_t = 8789)]
    port: u16,

    /// IP address to bind.
    #[arg(long, env = "MAJJJ_BIND", default_value = "0.0.0.0")]
    bind: IpAddr,

    /// Optional directory containing built frontend static assets (`web/dist`).
    #[arg(long, env = "MAJJJ_STATIC_DIR")]
    static_dir: Option<PathBuf>,

    /// Maximum worker threads for the dedicated Rayon AI thinking pool.
    #[arg(
        long,
        env = "MAJJJ_AI_THREADS",
        default_value_t = default_ai_threads(),
        value_parser = parse_ai_threads
    )]
    ai_threads: usize,
}

impl From<Cli> for ServerConfig {
    fn from(cli: Cli) -> Self {
        Self {
            bind: cli.bind,
            port: cli.port,
            static_dir: cli.static_dir,
            ai_threads: cli.ai_threads,
            ..Self::default()
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = ServerConfig::from(cli);

    let handle = match config.spawn().await {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    println!("majjj listening on http://{}", handle.addr());

    if let Err(err) = tokio::signal::ctrl_c().await {
        eprintln!("warning: signal listener error: {err}");
    }
    handle.shutdown().await;
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use clap::Parser;
    use mahjong_server::{default_ai_threads, ServerConfig};

    use super::Cli;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn cli_defaults_and_majjj_port_env_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_PORT");
        std::env::remove_var("MAJJJ_AI_THREADS");
        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse defaults");
        let default_cfg = ServerConfig::from(default_cli);
        assert_eq!(default_cfg.bind, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(default_cfg.port, 8789);
        assert_eq!(default_cfg.static_dir, None);
        assert_eq!(default_cfg.ai_threads, default_ai_threads());

        std::env::set_var("MAJJJ_PORT", "9876");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse with MAJJJ_PORT");
        std::env::remove_var("MAJJJ_PORT");
        let env_cfg = ServerConfig::from(env_cli);
        assert_eq!(env_cfg.port, 9876);
    }

    #[test]
    fn cli_accepts_port_bind_and_static_dir_overrides() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_PORT");
        std::env::remove_var("MAJJJ_AI_THREADS");
        let cli = Cli::try_parse_from([
            "mahjong-server",
            "--port",
            "9001",
            "--bind",
            "127.0.0.1",
            "--static-dir",
            "/tmp/web-dist",
        ])
        .expect("parse overrides");
        let cfg = ServerConfig::from(cli);
        assert_eq!(cfg.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(cfg.port, 9001);
        assert_eq!(cfg.static_dir, Some(PathBuf::from("/tmp/web-dist")));
    }

    #[test]
    fn cli_ai_threads_defaults_env_override_and_rejects_zero() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_AI_THREADS");

        // 1. Default computes (available_parallelism / 2).clamp(1, 4).
        let expected_default =
            std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4));
        assert_eq!(default_ai_threads(), expected_default);
        assert!((1..=4).contains(&default_ai_threads()));
        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse default cli");
        assert_eq!(ServerConfig::from(default_cli).ai_threads, expected_default);

        // 2. MAJJJ_AI_THREADS=2 sets ai_threads = 2.
        std::env::set_var("MAJJJ_AI_THREADS", "2");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse MAJJJ_AI_THREADS=2");
        assert_eq!(ServerConfig::from(env_cli).ai_threads, 2);

        // 3. CLI --ai-threads 3 overrides MAJJJ_AI_THREADS=2.
        let override_cli = Cli::try_parse_from(["mahjong-server", "--ai-threads", "3"])
            .expect("parse --ai-threads 3 over env");
        assert_eq!(ServerConfig::from(override_cli).ai_threads, 3);
        std::env::remove_var("MAJJJ_AI_THREADS");

        // 4. --ai-threads 3 works without env var.
        let flag_cli = Cli::try_parse_from(["mahjong-server", "--ai-threads", "3"])
            .expect("parse --ai-threads 3");
        assert_eq!(ServerConfig::from(flag_cli).ai_threads, 3);

        // 5. Reject 0 for both --ai-threads 0 and MAJJJ_AI_THREADS=0 with clear error message.
        let zero_flag_err = Cli::try_parse_from(["mahjong-server", "--ai-threads", "0"])
            .expect_err("must reject --ai-threads 0");
        let zero_flag_msg = zero_flag_err.to_string();
        assert!(
            zero_flag_msg.contains("at least 1") || zero_flag_msg.contains("0"),
            "unexpected error for --ai-threads 0: {zero_flag_msg}"
        );

        std::env::set_var("MAJJJ_AI_THREADS", "0");
        let zero_env_res = Cli::try_parse_from(["mahjong-server"]);
        std::env::remove_var("MAJJJ_AI_THREADS");
        let zero_env_err = zero_env_res.expect_err("must reject MAJJJ_AI_THREADS=0");
        let zero_env_msg = zero_env_err.to_string();
        assert!(
            zero_env_msg.contains("at least 1") || zero_env_msg.contains("0"),
            "unexpected error for MAJJJ_AI_THREADS=0: {zero_env_msg}"
        );
    }
}
