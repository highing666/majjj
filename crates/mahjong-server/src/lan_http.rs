//! Shared crate-private LAN HTTP/1.1 endpoint parser and request helper
//! used by both `llm` (`llama-server`) and `tts` (Windows LAN emotional TTS).

use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{HeaderMap, Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

/// Resolves an optional LAN service base URL with strict precedence:
/// CLI / env override > persisted `preferences.json` value > `default_url`.
#[must_use]
pub(crate) fn resolve_lan_base_url(
    cli_or_env: Option<&str>,
    persisted_pref: Option<&str>,
    default_url: &str,
) -> String {
    if let Some(url) = cli_or_env.map(str::trim).filter(|s| !s.is_empty()) {
        return url.to_string();
    }
    if let Some(url) = persisted_pref.map(str::trim).filter(|s| !s.is_empty()) {
        return url.to_string();
    }
    default_url.to_string()
}

/// Formats up to 160 characters of an HTTP error body for concise diagnostic messages.
#[must_use]
pub(crate) fn format_http_error_snippet(body: &[u8]) -> String {
    String::from_utf8_lossy(body)
        .trim()
        .chars()
        .take(160)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LanHttpError {
    InvalidEndpoint { endpoint: String, reason: String },
    Transport { endpoint: String, reason: String },
    Timeout { endpoint: String, timeout_ms: u64 },
}

/// Active streaming HTTP/1.1 response whose underlying connection is aborted on drop.
pub(crate) struct LanHttpStream {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    body: hyper::body::Incoming,
    conn_handle: tokio::task::JoinHandle<()>,
    endpoint: String,
}

impl LanHttpStream {
    pub(crate) async fn next_chunk(&mut self) -> Option<Result<Bytes, LanHttpError>> {
        loop {
            match self.body.frame().await {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        if !data.is_empty() {
                            return Some(Ok(data));
                        }
                    }
                }
                Some(Err(err)) => {
                    return Some(Err(LanHttpError::Transport {
                        endpoint: self.endpoint.clone(),
                        reason: err.to_string(),
                    }));
                }
                None => return None,
            }
        }
    }

    pub(crate) async fn collect_body(mut self, timeout: Duration) -> Result<Vec<u8>, LanHttpError> {
        let endpoint = self.endpoint.clone();
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let fut = async {
            let mut out = Vec::new();
            while let Some(chunk) = self.next_chunk().await {
                let bytes = chunk?;
                out.extend_from_slice(&bytes);
                if out.len() >= 16 * 1024 {
                    break;
                }
            }
            Ok(out)
        };
        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| LanHttpError::Timeout {
                endpoint,
                timeout_ms,
            })?
    }
}

impl Drop for LanHttpStream {
    fn drop(&mut self) {
        self.conn_handle.abort();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedEndpoint {
    pub(crate) raw_normalized: String,
    host: String,
    port: u16,
    host_header: String,
    base_path: String,
}

impl ParsedEndpoint {
    pub(crate) fn parse(raw: &str) -> Result<Self, LanHttpError> {
        let trimmed = raw.trim();
        let Some(rest) = trimmed.strip_prefix("http://") else {
            return Err(LanHttpError::InvalidEndpoint {
                endpoint: trimmed.to_string(),
                reason: "仅支持 http:// 局域网地址".to_string(),
            });
        };
        if rest.is_empty() {
            return Err(LanHttpError::InvalidEndpoint {
                endpoint: trimmed.to_string(),
                reason: "缺少主机地址".to_string(),
            });
        }

        let (authority, path_part) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, ""),
        };
        if authority.is_empty() || authority.contains('@') || authority.contains('?') {
            return Err(LanHttpError::InvalidEndpoint {
                endpoint: trimmed.to_string(),
                reason: "主机与端口格式不合法".to_string(),
            });
        }

        let (host, port, host_header) = if let Some(bracket_rest) = authority.strip_prefix('[') {
            let Some(end_bracket) = bracket_rest.find(']') else {
                return Err(LanHttpError::InvalidEndpoint {
                    endpoint: trimmed.to_string(),
                    reason: "IPv6 地址缺少右括号".to_string(),
                });
            };
            let ipv6_host = &bracket_rest[..end_bracket];
            if ipv6_host.is_empty() {
                return Err(LanHttpError::InvalidEndpoint {
                    endpoint: trimmed.to_string(),
                    reason: "IPv6 主机地址为空".to_string(),
                });
            }
            let after = &bracket_rest[end_bracket + 1..];
            let port = if after.is_empty() {
                80
            } else if let Some(port_str) = after.strip_prefix(':') {
                port_str
                    .parse::<u16>()
                    .map_err(|_| LanHttpError::InvalidEndpoint {
                        endpoint: trimmed.to_string(),
                        reason: format!("端口无效：{port_str}"),
                    })?
            } else {
                return Err(LanHttpError::InvalidEndpoint {
                    endpoint: trimmed.to_string(),
                    reason: "IPv6 端口语法不合法".to_string(),
                });
            };
            let host_header = if port == 80 {
                format!("[{ipv6_host}]")
            } else {
                format!("[{ipv6_host}]:{port}")
            };
            (ipv6_host.to_string(), port, host_header)
        } else {
            let (host_str, port) = match authority.rsplit_once(':') {
                Some((h, port_str)) if !h.contains(':') => {
                    let parsed_port =
                        port_str
                            .parse::<u16>()
                            .map_err(|_| LanHttpError::InvalidEndpoint {
                                endpoint: trimmed.to_string(),
                                reason: format!("端口无效：{port_str}"),
                            })?;
                    (h, parsed_port)
                }
                _ => (authority, 80),
            };
            if host_str.is_empty() || host_str.chars().any(char::is_whitespace) {
                return Err(LanHttpError::InvalidEndpoint {
                    endpoint: trimmed.to_string(),
                    reason: "主机名为空或包含非法空白字符".to_string(),
                });
            }
            let host_header = if port == 80 {
                host_str.to_string()
            } else {
                format!("{host_str}:{port}")
            };
            (host_str.to_string(), port, host_header)
        };

        let base_path = path_part.trim_end_matches('/').to_string();
        let raw_normalized = format!("http://{host_header}{base_path}");
        Ok(Self {
            raw_normalized,
            host,
            port,
            host_header,
            base_path,
        })
    }

    pub(crate) fn path_for(&self, suffix: &str) -> String {
        if self.base_path.is_empty() {
            suffix.to_string()
        } else {
            format!("{}{}", self.base_path, suffix)
        }
    }

    pub(crate) async fn send_http(
        &self,
        method: Method,
        suffix: &str,
        body: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>), LanHttpError> {
        let endpoint_str = self.raw_normalized.clone();
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let path = self.path_for(suffix);
        let host = self.host.clone();
        let port = self.port;
        let host_header = self.host_header.clone();

        let fut = async move {
            let (status, headers, incoming, _conn_handle) = send_request_headers(
                host,
                port,
                host_header,
                endpoint_str.clone(),
                method,
                path,
                body,
            )
            .await?;
            let collected = incoming
                .collect()
                .await
                .map_err(|err| LanHttpError::Transport {
                    endpoint: endpoint_str,
                    reason: err.to_string(),
                })?
                .to_bytes()
                .to_vec();
            Ok((status, headers, collected))
        };

        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| LanHttpError::Timeout {
                endpoint: self.raw_normalized.clone(),
                timeout_ms,
            })?
    }

    pub(crate) async fn send_http_stream(
        &self,
        method: Method,
        suffix: &str,
        body: Option<Vec<u8>>,
        header_timeout: Duration,
    ) -> Result<LanHttpStream, LanHttpError> {
        let endpoint_str = self.raw_normalized.clone();
        let timeout_ms = u64::try_from(header_timeout.as_millis()).unwrap_or(u64::MAX);
        let path = self.path_for(suffix);
        let host = self.host.clone();
        let port = self.port;
        let host_header = self.host_header.clone();

        let fut = async move {
            let (status, headers, incoming, conn_handle) = send_request_headers(
                host,
                port,
                host_header,
                endpoint_str.clone(),
                method,
                path,
                body,
            )
            .await?;
            Ok(LanHttpStream {
                status,
                headers,
                body: incoming,
                conn_handle,
                endpoint: endpoint_str,
            })
        };

        tokio::time::timeout(header_timeout, fut)
            .await
            .map_err(|_| LanHttpError::Timeout {
                endpoint: self.raw_normalized.clone(),
                timeout_ms,
            })?
    }
}

async fn send_request_headers(
    host: String,
    port: u16,
    host_header: String,
    endpoint_str: String,
    method: Method,
    path: String,
    body: Option<Vec<u8>>,
) -> Result<
    (
        StatusCode,
        HeaderMap,
        hyper::body::Incoming,
        tokio::task::JoinHandle<()>,
    ),
    LanHttpError,
> {
    let stream = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|err| LanHttpError::Transport {
            endpoint: endpoint_str.clone(),
            reason: err.to_string(),
        })?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|err| LanHttpError::Transport {
            endpoint: endpoint_str.clone(),
            reason: err.to_string(),
        })?;
    let conn_handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut builder = Request::builder()
        .method(method)
        .uri(&path)
        .header(HOST, host_header);
    let payload = match body {
        Some(bytes) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            Bytes::from(bytes)
        }
        None => Bytes::new(),
    };
    let req = builder
        .body(Full::new(payload))
        .map_err(|err| LanHttpError::Transport {
            endpoint: endpoint_str.clone(),
            reason: err.to_string(),
        })?;

    let res = sender
        .send_request(req)
        .await
        .map_err(|err| LanHttpError::Transport {
            endpoint: endpoint_str,
            reason: err.to_string(),
        })?;
    let status = res.status();
    let headers = res.headers().clone();
    let incoming = res.into_body();
    Ok((status, headers, incoming, conn_handle))
}
