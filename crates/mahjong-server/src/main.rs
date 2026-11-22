//! CLI entry point for `mahjong-server`.

use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use mahjong_server::ServerConfig;

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
}

impl From<Cli> for ServerConfig {
    fn from(cli: Cli) -> Self {
        Self {
            bind: cli.bind,
            port: cli.port,
            static_dir: cli.static_dir,
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

    use clap::Parser;
    use mahjong_server::ServerConfig;

    use super::Cli;

    #[test]
    fn cli_defaults_and_majjj_port_env_override() {
        std::env::remove_var("MAJJJ_PORT");
        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse defaults");
        let default_cfg = ServerConfig::from(default_cli);
        assert_eq!(default_cfg.bind, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(default_cfg.port, 8789);
        assert_eq!(default_cfg.static_dir, None);

        std::env::set_var("MAJJJ_PORT", "9876");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse with MAJJJ_PORT");
        std::env::remove_var("MAJJJ_PORT");
        let env_cfg = ServerConfig::from(env_cli);
        assert_eq!(env_cfg.port, 9876);
    }

    #[test]
    fn cli_accepts_port_bind_and_static_dir_overrides() {
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
}
