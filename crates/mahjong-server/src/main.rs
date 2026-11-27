//! CLI entry point for `mahjong-server`.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use mahjong_server::protocol::HealthResponse;
use mahjong_server::public_url::normalize_public_url;
use mahjong_server::storage::resolve_data_dir;
use mahjong_server::{default_ai_threads, ServerConfig, ServerError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn parse_ai_threads(raw: &str) -> Result<usize, String> {
    let parsed: usize = raw
        .parse()
        .map_err(|err| format!("invalid ai thread count '{raw}': {err}"))?;
    if parsed == 0 {
        return Err("ai thread count must be at least 1".to_string());
    }
    Ok(parsed)
}

fn parse_public_url(raw: &str) -> Result<String, String> {
    normalize_public_url(raw).map_err(|err| err.to_string())
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

    /// IP address to bind (defaults to dual-stack `[::]`, falling back to `0.0.0.0` if IPv6 is unavailable).
    #[arg(long, env = "MAJJJ_BIND")]
    bind: Option<IpAddr>,

    /// Optional directory containing built frontend static assets (`web/dist`).
    #[arg(long, env = "MAJJJ_STATIC_DIR")]
    static_dir: Option<PathBuf>,

    /// Directory for persistent JSONL match history and preferences.
    #[arg(long, env = "MAJJJ_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Maximum worker threads for the dedicated Rayon AI thinking pool.
    #[arg(
        long,
        env = "MAJJJ_AI_THREADS",
        default_value_t = default_ai_threads(),
        value_parser = parse_ai_threads
    )]
    ai_threads: usize,

    /// Skip automatically opening the browser on startup (desktop lifecycle remains enabled).
    #[arg(long, env = "MAJJJ_NO_OPEN", default_value_t = false)]
    no_open: bool,

    /// Run in 7x24 headless mode (implies `--no-open`; disables idle auto-exit and `/api/shutdown`).
    #[arg(long, env = "MAJJJ_HEADLESS", default_value_t = false)]
    headless: bool,

    /// Base URL for the mandatory LAN OpenAI-compatible `llama-server` endpoint.
    #[arg(long, env = "MAJJJ_LLM_URL")]
    llm_url: Option<String>,

    /// Base URL for the optional LAN emotional TTS service endpoint.
    #[arg(long, env = "MAJJJ_TTS_URL")]
    tts_url: Option<String>,

    /// Explicitly disable LAN emotional TTS synthesis (`BanterVoice`).
    #[arg(
        long,
        env = "MAJJJ_NO_TTS",
        default_value_t = false,
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    no_tts: bool,

    /// Custom public origin override for remote invitation links (e.g. `http://majjj.example.com:8789`).
    #[arg(long, env = "MAJJJ_PUBLIC_URL", value_parser = parse_public_url)]
    public_url: Option<String>,
}

impl From<Cli> for ServerConfig {
    fn from(cli: Cli) -> Self {
        let mut cfg = Self::default()
            .with_port(cli.port)
            .with_static_dir(cli.static_dir)
            .with_data_dir(resolve_data_dir(cli.data_dir))
            .with_ai_threads(cli.ai_threads)
            .with_no_open(cli.no_open || cli.headless)
            .with_headless(cli.headless)
            .with_llm_url(cli.llm_url)
            .with_tts_url(cli.tts_url)
            .with_no_tts(cli.no_tts)
            .with_public_url(cli.public_url);
        if let Some(bind) = cli.bind {
            cfg = cfg.with_bind(bind);
        }
        cfg
    }
}

fn open_browser_url(url: &str) {
    #[cfg(target_os = "macos")]
    let res = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let res = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let res: std::io::Result<std::process::Child> = Err(std::io::Error::new(
        ErrorKind::Unsupported,
        "automatic browser launch is only supported on macOS and Linux",
    ));

    if let Err(err) = res {
        eprintln!("warning: failed to open browser at {url}: {err}");
    }
}

async fn probe_existing_majjj_health(port: u16) -> Option<HealthResponse> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(addr))
        .await
        .ok()?
        .ok()?;
    let request =
        format!("GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    tokio::time::timeout(Duration::from_secs(2), stream.write_all(request.as_bytes()))
        .await
        .ok()?
        .ok()?;

    let mut raw = Vec::new();
    let mut buf = [0_u8; 4096];
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        while raw.len() < 32 * 1024 {
            match stream.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => raw.extend_from_slice(&buf[..n]),
            }
        }
    })
    .await;

    parse_http_health_response(&raw)
}

fn parse_http_health_response(raw: &[u8]) -> Option<HealthResponse> {
    let text = std::str::from_utf8(raw).ok()?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))?;
    let status_line = head.lines().next()?;
    if !status_line.contains(" 200 ") {
        return None;
    }
    let is_chunked = head.lines().any(|line| {
        line.to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
    });
    let json_body = if is_chunked {
        decode_chunked_body(body)?
    } else {
        body.to_string()
    };
    let parsed: HealthResponse = serde_json::from_str(json_body.trim()).ok()?;
    (parsed.app == "majjj").then_some(parsed)
}

fn decode_chunked_body(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = raw;
    loop {
        let line_end = rest.find("\r\n")?;
        let size_hex = rest[..line_end].split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        if rest.len() < size + 2 {
            return None;
        }
        out.push_str(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

async fn run_cli(config: ServerConfig) -> ExitCode {
    let port = config.port;
    let no_open = config.no_open;
    let headless = config.headless;

    let mut handle = match config.spawn().await {
        Ok(handle) => handle,
        Err(ServerError::Bind { source, .. })
            if source.kind() == ErrorKind::AddrInUse && !headless =>
        {
            if let Some(health) = probe_existing_majjj_health(port).await {
                let url = format!("http://127.0.0.1:{}", health.port);
                println!("majjj is already running at {url}");
                if !no_open {
                    open_browser_url(&url);
                }
                return ExitCode::SUCCESS;
            }
            eprintln!(
                "error: port {port} is already in use by another process (127.0.0.1:{port}/api/health did not confirm majjj)"
            );
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    println!("majjj listening on http://{}", handle.addr());
    if !no_open {
        let url = format!("http://127.0.0.1:{}", handle.addr().port());
        open_browser_url(&url);
    }

    tokio::select! {
        res = handle.wait() => {
            if let Err(err) = res {
                eprintln!("error: server exited with error: {err}");
                return ExitCode::FAILURE;
            }
        }
        res = tokio::signal::ctrl_c() => {
            if let Err(err) = res {
                eprintln!("warning: signal listener error: {err}");
            }
            handle.shutdown().await;
        }
    }
    ExitCode::SUCCESS
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = ServerConfig::from(cli);
    run_cli(config).await
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use clap::Parser;
    use mahjong_server::protocol::TablePhase;
    use mahjong_server::{default_ai_threads, ServerConfig};

    use super::{parse_http_health_response, Cli};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn cli_defaults_and_majjj_port_env_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_BIND");
        std::env::remove_var("MAJJJ_PORT");
        std::env::remove_var("MAJJJ_AI_THREADS");
        std::env::remove_var("MAJJJ_NO_OPEN");
        std::env::remove_var("MAJJJ_HEADLESS");
        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse defaults");
        let default_cfg = ServerConfig::from(default_cli);
        assert_eq!(default_cfg.bind, IpAddr::V6(Ipv6Addr::UNSPECIFIED));
        assert!(!default_cfg.is_bind_explicit());
        assert_eq!(default_cfg.port, 8789);
        assert_eq!(default_cfg.static_dir, None);
        assert_eq!(default_cfg.ai_threads, default_ai_threads());
        assert!(!default_cfg.no_open);
        assert!(!default_cfg.headless);

        std::env::set_var("MAJJJ_PORT", "9876");
        std::env::set_var("MAJJJ_BIND", "::");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse with MAJJJ_PORT");
        std::env::remove_var("MAJJJ_PORT");
        std::env::remove_var("MAJJJ_BIND");
        let env_cfg = ServerConfig::from(env_cli);
        assert_eq!(env_cfg.port, 9876);
        assert_eq!(env_cfg.bind, IpAddr::V6(Ipv6Addr::UNSPECIFIED));
        assert!(env_cfg.is_bind_explicit());
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

    #[test]
    fn cli_data_dir_resolves_default_env_and_flag_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_DATA_DIR");

        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse default data_dir");
        let default_cfg = ServerConfig::from(default_cli);
        assert_eq!(
            default_cfg.data_dir,
            Some(mahjong_server::storage::resolve_default_data_dir())
        );

        std::env::set_var("MAJJJ_DATA_DIR", "/tmp/majjj-env-data");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse MAJJJ_DATA_DIR");
        std::env::remove_var("MAJJJ_DATA_DIR");
        assert_eq!(
            ServerConfig::from(env_cli).data_dir,
            Some(PathBuf::from("/tmp/majjj-env-data"))
        );

        let flag_cli =
            Cli::try_parse_from(["mahjong-server", "--data-dir", "/tmp/majjj-flag-data"])
                .expect("parse --data-dir");
        assert_eq!(
            ServerConfig::from(flag_cli).data_dir,
            Some(PathBuf::from("/tmp/majjj-flag-data"))
        );
    }

    #[test]
    fn cli_accepts_no_open_and_headless_flags_and_headless_implies_no_open() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_NO_OPEN");
        std::env::remove_var("MAJJJ_HEADLESS");

        let no_open_cli =
            Cli::try_parse_from(["mahjong-server", "--no-open"]).expect("parse --no-open");
        let no_open_cfg = ServerConfig::from(no_open_cli);
        assert!(no_open_cfg.no_open);
        assert!(!no_open_cfg.headless);

        let headless_cli =
            Cli::try_parse_from(["mahjong-server", "--headless"]).expect("parse --headless");
        let headless_cfg = ServerConfig::from(headless_cli);
        assert!(headless_cfg.headless);
        assert!(headless_cfg.no_open, "--headless must imply no_open = true");

        std::env::set_var("MAJJJ_HEADLESS", "true");
        let env_headless_cli =
            Cli::try_parse_from(["mahjong-server"]).expect("parse MAJJJ_HEADLESS=true");
        std::env::remove_var("MAJJJ_HEADLESS");
        let env_headless_cfg = ServerConfig::from(env_headless_cli);
        assert!(env_headless_cfg.headless);
        assert!(env_headless_cfg.no_open);
    }

    #[test]
    fn cli_accepts_llm_url_flag_and_env_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_LLM_URL");

        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse default llm_url");
        assert_eq!(ServerConfig::from(default_cli).llm_url, None);

        std::env::set_var("MAJJJ_LLM_URL", "http://192.168.5.77:8080");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse MAJJJ_LLM_URL");
        assert_eq!(
            ServerConfig::from(env_cli).llm_url.as_deref(),
            Some("http://192.168.5.77:8080")
        );

        let flag_cli =
            Cli::try_parse_from(["mahjong-server", "--llm-url", "http://127.0.0.1:18080"])
                .expect("parse --llm-url over env");
        std::env::remove_var("MAJJJ_LLM_URL");
        assert_eq!(
            ServerConfig::from(flag_cli).llm_url.as_deref(),
            Some("http://127.0.0.1:18080")
        );
    }

    #[test]
    fn cli_accepts_tts_url_and_no_tts_flag_and_truthy_env_overrides() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_TTS_URL");
        std::env::remove_var("MAJJJ_NO_TTS");

        let default_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse defaults");
        let default_cfg = ServerConfig::from(default_cli);
        assert_eq!(default_cfg.tts_url, None);
        assert!(!default_cfg.no_tts);

        std::env::set_var("MAJJJ_TTS_URL", "http://192.168.5.77:8081");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse MAJJJ_TTS_URL");
        assert_eq!(
            ServerConfig::from(env_cli).tts_url.as_deref(),
            Some("http://192.168.5.77:8081")
        );

        let flag_cli =
            Cli::try_parse_from(["mahjong-server", "--tts-url", "http://127.0.0.1:18081"])
                .expect("parse --tts-url over env");
        std::env::remove_var("MAJJJ_TTS_URL");
        assert_eq!(
            ServerConfig::from(flag_cli).tts_url.as_deref(),
            Some("http://127.0.0.1:18081")
        );

        for truthy in ["1", "true", "yes", "on"] {
            std::env::set_var("MAJJJ_NO_TTS", truthy);
            let parsed = Cli::try_parse_from(["mahjong-server"]);
            std::env::remove_var("MAJJJ_NO_TTS");
            let parsed = parsed.unwrap_or_else(|err| panic!("parse MAJJJ_NO_TTS={truthy}: {err}"));
            assert!(
                ServerConfig::from(parsed).no_tts,
                "MAJJJ_NO_TTS={truthy} must disable TTS"
            );
        }
        for falsy in ["0", "false", "no", "off"] {
            std::env::set_var("MAJJJ_NO_TTS", falsy);
            let parsed = Cli::try_parse_from(["mahjong-server"]);
            std::env::remove_var("MAJJJ_NO_TTS");
            let parsed = parsed.unwrap_or_else(|err| panic!("parse MAJJJ_NO_TTS={falsy}: {err}"));
            assert!(
                !ServerConfig::from(parsed).no_tts,
                "MAJJJ_NO_TTS={falsy} must keep TTS enabled"
            );
        }

        let no_tts_flag =
            Cli::try_parse_from(["mahjong-server", "--no-tts"]).expect("parse --no-tts");
        assert!(ServerConfig::from(no_tts_flag).no_tts);
    }

    #[test]
    fn cli_accepts_public_url_flag_and_env_override_and_rejects_invalid_origins() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("MAJJJ_PUBLIC_URL");

        let default_cli =
            Cli::try_parse_from(["mahjong-server"]).expect("parse default public_url");
        assert_eq!(ServerConfig::from(default_cli).public_url, None);

        std::env::set_var("MAJJJ_PUBLIC_URL", "HTTPS://Env.Example.COM:8443/");
        let env_cli = Cli::try_parse_from(["mahjong-server"]).expect("parse MAJJJ_PUBLIC_URL");
        assert_eq!(
            ServerConfig::from(env_cli).public_url.as_deref(),
            Some("https://env.example.com:8443")
        );

        let flag_cli = Cli::try_parse_from([
            "mahjong-server",
            "--public-url",
            "http://majjj.example.com:8789/",
        ])
        .expect("parse --public-url over env");
        std::env::remove_var("MAJJJ_PUBLIC_URL");
        assert_eq!(
            ServerConfig::from(flag_cli).public_url.as_deref(),
            Some("http://majjj.example.com:8789")
        );

        for invalid in [
            "ftp://majjj.example.com",
            "http://majjj.example.com:8789/table",
            "http://user:pass@majjj.example.com:8789",
            "http://2001:db9::1:8789",
            "",
        ] {
            let err = Cli::try_parse_from(["mahjong-server", "--public-url", invalid])
                .expect_err(&format!("must reject invalid --public-url {invalid:?}"));
            let msg = err.to_string();
            assert!(
                msg.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "CLI error for --public-url {invalid:?} must contain clear Chinese reason: {msg}"
            );
        }

        std::env::set_var("MAJJJ_PUBLIC_URL", "http://majjj.example.com/bad");
        let env_err = Cli::try_parse_from(["mahjong-server"]);
        std::env::remove_var("MAJJJ_PUBLIC_URL");
        assert!(
            env_err.is_err(),
            "invalid MAJJJ_PUBLIC_URL must be rejected"
        );
    }

    #[test]
    fn parse_http_health_response_validates_majjj_app_and_rejects_other_services() {
        let ok_raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"app\":\"majjj\",\"version\":\"0.1.0\",\"phase\":\"Lobby\",\"online_humans\":0,\"port\":8789}";
        let health = parse_http_health_response(ok_raw).expect("valid majjj health");
        assert_eq!(health.app, "majjj");
        assert_eq!(health.port, 8789);
        assert_eq!(health.phase, TablePhase::Lobby);

        let body = "{\"app\":\"majjj\",\"version\":\"0.1.0\",\"phase\":\"InMatch\",\"online_humans\":2,\"port\":8789}";
        let chunked = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );
        let chunked_health =
            parse_http_health_response(chunked.as_bytes()).expect("chunked majjj health");
        assert_eq!(chunked_health.phase, TablePhase::InMatch);
        assert_eq!(chunked_health.online_humans, 2);

        let other_app = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"app\":\"other\",\"version\":\"1.0.0\",\"phase\":\"Lobby\",\"online_humans\":0,\"port\":8789}";
        assert!(parse_http_health_response(other_app).is_none());

        let not_found = b"HTTP/1.1 404 Not Found\r\n\r\n";
        assert!(parse_http_health_response(not_found).is_none());
    }
}
