//! Narrow asynchronous `TtsClient` abstraction and HTTP/1.1 client for the optional
//! LAN Windows emotional TTS service (`BanterVoice` / `台词语音`).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use hyper::{HeaderMap, Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::lan_http::{
    format_http_error_snippet, resolve_lan_base_url, LanHttpError, ParsedEndpoint,
};
use crate::persona::{PersonaId, PersonaMood};

/// Default LAN Windows emotional TTS endpoint (see Spec #81 / ADR-0011).
pub const DEFAULT_TTS_BASE_URL: &str = "http://192.168.5.50:8081";

/// Synthesis deadline for in-hand `BanterVoice` lines (`5,000 ms` per ADR-0014).
pub const TTS_IN_HAND_DEADLINE_MS: u32 = 5_000;

/// Synthesis deadline for hand-settlement `BanterVoice` lines (`6,500 ms` per ADR-0014).
pub const TTS_SETTLEMENT_DEADLINE_MS: u32 = 6_500;

/// Cooldown backoff duration before probing `/health` after TTS unavailability (`15 s` per Spec #81).
pub const TTS_COOLDOWN_DURATION: Duration = Duration::from_secs(15);

const DEFAULT_HEALTH_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

/// Boxed `Send` future returned by [`TtsClient`] methods.
pub type TtsFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Structured request sent to `POST /v1/tts/synthesize` on the LAN emotional TTS service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsSynthesisRequest {
    /// Monotonic request / banter identifier (`banter_id`).
    pub request_id: String,
    /// Speaking AI `PersonaId` (`牌友角色`).
    pub persona_id: PersonaId,
    /// Discrete `PersonaMood` (`牌友情绪`) driving vocal expression.
    pub mood: PersonaMood,
    /// Single-line Chinese banter text (`牌桌台词`) to synthesize.
    pub text: String,
    /// Optional explicit style instruction override (`None` uses persona+mood defaults).
    pub instruct_override: Option<String>,
    /// Output audio format (`"wav"` in v1).
    pub format: String,
    /// Streaming mode flag (`false` in v1; reserved for v1.5 raw PCM streaming).
    pub stream: bool,
    /// End-to-end synthesis deadline in milliseconds (`2500` in-hand, `5000` settlement).
    pub deadline_ms: u32,
}

impl TtsSynthesisRequest {
    /// Builds a v1 non-streaming 24 kHz WAV synthesis request.
    #[must_use]
    pub fn new(
        request_id: impl Into<String>,
        persona_id: PersonaId,
        mood: PersonaMood,
        text: impl Into<String>,
        deadline_ms: u32,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            persona_id,
            mood,
            text: text.into(),
            instruct_override: None,
            format: "wav".to_string(),
            stream: false,
            deadline_ms,
        }
    }
}

/// Synthesized 24 kHz mono WAV output returned by [`TtsClient::synthesize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsAudioOutput {
    /// Complete RIFF/WAVE binary payload.
    pub wav_bytes: Vec<u8>,
    /// Audio clip duration in milliseconds.
    pub duration_ms: u32,
}

/// Successful `/health` probe result returned by [`TtsClient::health_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsHealthStatus {
    /// Normalized endpoint base URL.
    pub endpoint: String,
    /// Active TTS backend identifier reported by `/health`, if present.
    pub backend: Option<String>,
    /// Active TTS model identifier reported by `/health`, if present.
    pub model: Option<String>,
}

/// Errors produced by [`TtsClient`] probing or synthesis calls.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TtsError {
    /// Configured base URL is malformed or uses an unsupported scheme.
    #[error("语音合成服务地址无效（{endpoint}）：{reason}")]
    InvalidEndpoint { endpoint: String, reason: String },
    /// TCP connection or HTTP transport error.
    #[error("无法连接到语音合成服务（{endpoint}）：{reason}")]
    Transport { endpoint: String, reason: String },
    /// Request exceeded the configured timeout.
    #[error("语音合成服务请求超时（{endpoint}，超过 {timeout_ms}ms）")]
    Timeout { endpoint: String, timeout_ms: u64 },
    /// Upstream server dropped the request because `deadline_ms` was exceeded before synthesis.
    #[error("语音合成请求超过时限被丢弃（{endpoint}）")]
    DeadlineExceeded { endpoint: String },
    /// HTTP status code was not 2xx.
    #[error("语音合成服务返回异常状态码 HTTP {status}（{endpoint}）：{body}")]
    HttpStatus {
        endpoint: String,
        status: u16,
        body: String,
    },
    /// Response payload was not valid WAV or failed `/health` readiness validation.
    #[error("语音合成服务返回格式异常：{reason}")]
    InvalidResponse { reason: String },
}

impl TtsError {
    /// Returns `true` when this error indicates the LAN TTS service itself is unhealthy
    /// or unreachable (eligible for circuit-breaker counting), and `false` for per-line
    /// latency/queue shedding (`Timeout`, HTTP 408 `DeadlineExceeded`) per ADR-0014.
    #[must_use]
    pub const fn is_service_outage(&self) -> bool {
        !matches!(self, Self::Timeout { .. } | Self::DeadlineExceeded { .. })
    }
}

/// Narrow asynchronous boundary for talking to the optional LAN emotional TTS service.
///
/// Production code uses [`HttpTtsClient`]; tests inject local fake HTTP servers
/// or deterministic in-memory stubs through `ServerConfig::with_tts_client` or
/// `TableOptions::with_tts_client`.
pub trait TtsClient: Send + Sync + std::fmt::Debug {
    /// Returns the human-readable endpoint description for diagnostics and errors.
    fn endpoint(&self) -> &str;

    /// Probes `GET /health` to verify the TTS service is ready.
    fn health_check(&self) -> TtsFuture<'_, Result<TtsHealthStatus, TtsError>>;

    /// Synthesizes one `BanterVoice` WAV clip via `POST /v1/tts/synthesize`.
    fn synthesize(
        &self,
        request: TtsSynthesisRequest,
    ) -> TtsFuture<'_, Result<TtsAudioOutput, TtsError>>;
}

impl From<LanHttpError> for TtsError {
    fn from(err: LanHttpError) -> Self {
        match err {
            LanHttpError::InvalidEndpoint { endpoint, reason } => {
                Self::InvalidEndpoint { endpoint, reason }
            }
            LanHttpError::Transport { endpoint, reason } => Self::Transport { endpoint, reason },
            LanHttpError::Timeout {
                endpoint,
                timeout_ms,
            } => Self::Timeout {
                endpoint,
                timeout_ms,
            },
        }
    }
}

/// Resolves the effective TTS base URL with strict precedence:
/// CLI `--tts-url` / `MAJJJ_TTS_URL` > persisted `preferences.json` `tts_base_url` > [`DEFAULT_TTS_BASE_URL`].
#[must_use]
pub fn resolve_tts_base_url(cli_or_env: Option<&str>, persisted_pref: Option<&str>) -> String {
    resolve_lan_base_url(cli_or_env, persisted_pref, DEFAULT_TTS_BASE_URL)
}

/// Parses a standard RIFF/WAVE binary payload and returns its duration in milliseconds.
///
/// Returns `None` if `wav_bytes` is not a valid RIFF/WAVE buffer with `fmt ` and `data` chunks.
#[must_use]
pub fn parse_wav_duration_ms(wav_bytes: &[u8]) -> Option<u32> {
    if wav_bytes.len() < 12 || &wav_bytes[0..4] != b"RIFF" || &wav_bytes[8..12] != b"WAVE" {
        return None;
    }

    let mut offset = 12usize;
    let mut byte_rate: Option<u32> = None;
    let mut data_size: Option<u32> = None;

    while offset + 8 <= wav_bytes.len() {
        let chunk_id = &wav_bytes[offset..offset + 4];
        let chunk_len = u32::from_le_bytes([
            wav_bytes[offset + 4],
            wav_bytes[offset + 5],
            wav_bytes[offset + 6],
            wav_bytes[offset + 7],
        ]) as usize;
        let payload_start = offset + 8;
        let payload_end = payload_start.saturating_add(chunk_len).min(wav_bytes.len());

        if chunk_id == b"fmt " && payload_end.saturating_sub(payload_start) >= 16 {
            let channels =
                u16::from_le_bytes([wav_bytes[payload_start + 2], wav_bytes[payload_start + 3]]);
            let sample_rate = u32::from_le_bytes([
                wav_bytes[payload_start + 4],
                wav_bytes[payload_start + 5],
                wav_bytes[payload_start + 6],
                wav_bytes[payload_start + 7],
            ]);
            let header_byte_rate = u32::from_le_bytes([
                wav_bytes[payload_start + 8],
                wav_bytes[payload_start + 9],
                wav_bytes[payload_start + 10],
                wav_bytes[payload_start + 11],
            ]);
            let bits_per_sample =
                u16::from_le_bytes([wav_bytes[payload_start + 14], wav_bytes[payload_start + 15]]);
            let computed_rate = sample_rate
                .saturating_mul(u32::from(channels))
                .saturating_mul(u32::from(bits_per_sample / 8));
            let effective_rate = if header_byte_rate > 0 {
                header_byte_rate
            } else {
                computed_rate
            };
            if effective_rate > 0 {
                byte_rate = Some(effective_rate);
            }
        } else if chunk_id == b"data" {
            let available_data = payload_end.saturating_sub(payload_start);
            data_size = Some(u32::try_from(available_data).unwrap_or(u32::MAX));
        }

        // RIFF chunks are word-aligned (2-byte padded on odd sizes).
        let padded_len = chunk_len.saturating_add(chunk_len & 1);
        let Some(next) = payload_start.checked_add(padded_len) else {
            break;
        };
        if next <= offset {
            break;
        }
        offset = next;
    }

    let rate = u64::from(byte_rate?);
    let data = u64::from(data_size?);
    let ms = (data.saturating_mul(1000).saturating_add(rate / 2)) / rate;
    Some(u32::try_from(ms).unwrap_or(u32::MAX))
}

/// Production HTTP/1.1 client targeting the Windows LAN emotional TTS service.
pub struct HttpTtsClient {
    endpoint: ParsedEndpoint,
    health_timeout: Duration,
    request_timeout: Duration,
}

impl std::fmt::Debug for HttpTtsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTtsClient")
            .field("endpoint", &self.endpoint.raw_normalized)
            .field("health_timeout", &self.health_timeout)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

impl HttpTtsClient {
    /// Creates a new HTTP/1.1 TTS client for the given `http://...` base URL.
    ///
    /// # Errors
    ///
    /// Returns [`TtsError::InvalidEndpoint`] if `base_url` is malformed.
    pub fn new(base_url: &str) -> Result<Self, TtsError> {
        let endpoint = ParsedEndpoint::parse(base_url)?;
        Ok(Self {
            endpoint,
            health_timeout: DEFAULT_HEALTH_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    /// Overrides the timeout used for `GET /health` probes.
    #[must_use]
    pub fn with_health_timeout(mut self, timeout: Duration) -> Self {
        self.health_timeout = timeout;
        self
    }

    /// Overrides the ceiling timeout used for `POST /v1/tts/synthesize` requests.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    async fn send_http(
        &self,
        method: Method,
        suffix: &str,
        body: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>), TtsError> {
        Ok(self
            .endpoint
            .send_http(method, suffix, body, timeout)
            .await?)
    }

    fn ensure_http_success(&self, status: StatusCode, body: &[u8]) -> Result<(), TtsError> {
        if status.is_success() {
            return Ok(());
        }
        if status == StatusCode::REQUEST_TIMEOUT {
            return Err(TtsError::DeadlineExceeded {
                endpoint: self.endpoint.raw_normalized.clone(),
            });
        }
        Err(TtsError::HttpStatus {
            endpoint: self.endpoint.raw_normalized.clone(),
            status: status.as_u16(),
            body: format_http_error_snippet(body),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TtsHealthBody {
    status: Option<String>,
    ready: Option<bool>,
    backend: Option<String>,
    model: Option<String>,
}

fn extract_duration_ms(headers: &HeaderMap, wav_bytes: &[u8]) -> Result<u32, TtsError> {
    let header_duration = headers
        .get("x-tts-duration-ms")
        .and_then(|val| val.to_str().ok())
        .and_then(|s| s.trim().parse::<u32>().ok());
    let parsed_wav = parse_wav_duration_ms(wav_bytes);
    if parsed_wav.is_none() {
        return Err(TtsError::InvalidResponse {
            reason: "响应体不是有效的 RIFF/WAVE 音频".to_string(),
        });
    }
    Ok(header_duration.unwrap_or_else(|| parsed_wav.unwrap_or(0)))
}

impl TtsClient for HttpTtsClient {
    fn endpoint(&self) -> &str {
        &self.endpoint.raw_normalized
    }

    fn health_check(&self) -> TtsFuture<'_, Result<TtsHealthStatus, TtsError>> {
        Box::pin(async move {
            let (status, _headers, body) = self
                .send_http(Method::GET, "/health", None, self.health_timeout)
                .await?;
            self.ensure_http_success(status, &body)?;

            let mut backend = None;
            let mut model = None;
            if let Ok(parsed) = serde_json::from_slice::<TtsHealthBody>(&body) {
                if parsed.ready == Some(false) {
                    return Err(TtsError::InvalidResponse {
                        reason: "/health 返回 ready=false".to_string(),
                    });
                }
                if let Some(status_text) = parsed.status.as_deref() {
                    if !status_text.eq_ignore_ascii_case("ok") {
                        return Err(TtsError::InvalidResponse {
                            reason: format!("/health 状态非就绪：{status_text}"),
                        });
                    }
                }
                backend = parsed.backend;
                model = parsed.model;
            }

            Ok(TtsHealthStatus {
                endpoint: self.endpoint.raw_normalized.clone(),
                backend,
                model,
            })
        })
    }

    fn synthesize(
        &self,
        request: TtsSynthesisRequest,
    ) -> TtsFuture<'_, Result<TtsAudioOutput, TtsError>> {
        Box::pin(async move {
            let payload =
                serde_json::to_vec(&request).map_err(|err| TtsError::InvalidResponse {
                    reason: format!("序列化语音合成请求失败：{err}"),
                })?;
            let (status, headers, body) = self
                .send_http(
                    Method::POST,
                    "/v1/tts/synthesize",
                    Some(payload),
                    self.request_timeout,
                )
                .await?;
            self.ensure_http_success(status, &body)?;
            let duration_ms = extract_duration_ms(&headers, &body)?;
            Ok(TtsAudioOutput {
                wav_bytes: body,
                duration_ms,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use hyper::header::HeaderValue;
    use hyper::HeaderMap;

    use super::{
        extract_duration_ms, parse_wav_duration_ms, resolve_tts_base_url, HttpTtsClient, TtsError,
        TtsSynthesisRequest, DEFAULT_TTS_BASE_URL, TTS_IN_HAND_DEADLINE_MS,
    };
    use crate::persona::{PersonaId, PersonaMood};

    fn make_pcm16_mono_wav(sample_rate: u32, num_samples: usize) -> Vec<u8> {
        let data_len = (num_samples * 2) as u32;
        let riff_len = 36 + data_len;
        let byte_rate = sample_rate * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&riff_len.to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1_u16.to_le_bytes()); // mono
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&2_u16.to_le_bytes()); // block align
        out.extend_from_slice(&16_u16.to_le_bytes()); // bits per sample
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.resize(44 + data_len as usize, 0);
        out
    }

    #[test]
    fn resolve_tts_base_url_follows_cli_over_prefs_over_default() {
        assert_eq!(resolve_tts_base_url(None, None), DEFAULT_TTS_BASE_URL);
        assert_eq!(
            resolve_tts_base_url(Some("  "), Some("")),
            DEFAULT_TTS_BASE_URL
        );
        assert_eq!(
            resolve_tts_base_url(None, Some("http://192.168.5.99:8081")),
            "http://192.168.5.99:8081"
        );
        assert_eq!(
            resolve_tts_base_url(
                Some("http://127.0.0.1:18081"),
                Some("http://192.168.5.99:8081")
            ),
            "http://127.0.0.1:18081"
        );
    }

    #[test]
    fn http_tts_client_rejects_malformed_or_non_http_endpoints() {
        assert!(matches!(
            HttpTtsClient::new("https://192.168.5.50:8081"),
            Err(TtsError::InvalidEndpoint { .. })
        ));
        assert!(matches!(
            HttpTtsClient::new("192.168.5.50:8081"),
            Err(TtsError::InvalidEndpoint { .. })
        ));
        assert!(matches!(
            HttpTtsClient::new("http://"),
            Err(TtsError::InvalidEndpoint { .. })
        ));
        assert!(HttpTtsClient::new("http://192.168.5.50:8081/").is_ok());
    }

    #[test]
    fn parse_wav_duration_ms_computes_exact_duration_and_rejects_non_wav() {
        // 24 kHz mono PCM16: 12,000 samples == 500 ms.
        let wav_500ms = make_pcm16_mono_wav(24_000, 12_000);
        assert_eq!(parse_wav_duration_ms(&wav_500ms), Some(500));
        assert_eq!(parse_wav_duration_ms(b"not a wav"), None);

        // Prefers X-TTS-Duration-Ms header when present while still validating WAV magic.
        let mut headers = HeaderMap::new();
        headers.insert("x-tts-duration-ms", HeaderValue::from_static("640"));
        assert_eq!(extract_duration_ms(&headers, &wav_500ms).unwrap(), 640);
        assert!(extract_duration_ms(&headers, b"garbage").is_err());

        // Falls back to parsed WAV header duration when X-TTS-Duration-Ms is absent.
        assert_eq!(
            extract_duration_ms(&HeaderMap::new(), &wav_500ms).unwrap(),
            500
        );
    }

    #[test]
    fn synthesize_request_serializes_exact_windows_tts_contract() {
        let req = TtsSynthesisRequest::new(
            "b-7",
            PersonaId::QuickHothead,
            PersonaMood::Tilted,
            "这都能被你碰走？",
            TTS_IN_HAND_DEADLINE_MS,
        );
        let value = serde_json::to_value(&req).expect("serialize request");
        assert_eq!(value["request_id"], "b-7");
        assert_eq!(value["persona_id"], "QuickHothead");
        assert_eq!(value["mood"], "Tilted");
        assert_eq!(value["text"], "这都能被你碰走？");
        assert!(value["instruct_override"].is_null());
        assert_eq!(value["format"], "wav");
        assert_eq!(value["stream"], false);
        assert_eq!(value["deadline_ms"], 5000);
    }
}
