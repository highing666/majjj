//! Narrow asynchronous `LlmClient` abstraction and OpenAI-compatible HTTP client
//! for the mandatory LAN Windows `llama-server` inference dependency.

use std::future::Future;
use std::pin::Pin;
use std::sync::RwLock;
use std::time::Duration;

use hyper::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::lan_http::{
    format_http_error_snippet, resolve_lan_base_url, LanHttpError, ParsedEndpoint,
};
use crate::persona::PersonaMood;
use crate::persona_memory::normalize_grudge_note;
use crate::storage::normalize_single_line_text;

/// Default LAN Windows `llama-server` endpoint (see Spec #63 / #65).
pub const DEFAULT_LLM_BASE_URL: &str = "http://192.168.5.50:8080";

const DEFAULT_HEALTH_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
const DEFAULT_TEMPERATURE: f32 = 0.9;
const DEFAULT_TOP_P: f32 = 0.9;
const DEFAULT_PRESENCE_PENALTY: f32 = 0.4;
const DEFAULT_REPEAT_PENALTY: f32 = 1.1;
const DEFAULT_MAX_TOKENS: u32 = 96;

/// Boxed `Send` future returned by [`LlmClient`] methods.
pub type LlmFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Structured request for generating one persona banter line (`牌桌台词`).
#[derive(Debug, Clone, PartialEq)]
pub struct BanterGenerationRequest {
    /// System prompt establishing persona identity, 6 mood anchors, and output rules.
    pub system_prompt: String,
    /// User prompt describing the current highlight event and privacy-safe table facts.
    pub user_prompt: String,
    /// Sampling temperature (`#65` default: `0.9`).
    pub temperature: f32,
    /// Nucleus sampling `top_p` (`#65` default: `0.9`).
    pub top_p: f32,
    /// Presence penalty encouraging varied phrasing (`#65` default: `0.4`).
    pub presence_penalty: f32,
    /// Repeat penalty discouraging repetitive phrasing (`#65` default: `1.1`).
    pub repeat_penalty: f32,
    /// Maximum completion tokens for a concise single-line JSON output.
    pub max_tokens: u32,
}

impl BanterGenerationRequest {
    /// Builds a banter generation request using `#65`-tuned sampling defaults.
    #[must_use]
    pub fn new(system_prompt: impl Into<String>, user_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            user_prompt: user_prompt.into(),
            temperature: DEFAULT_TEMPERATURE,
            top_p: DEFAULT_TOP_P,
            presence_penalty: DEFAULT_PRESENCE_PENALTY,
            repeat_penalty: DEFAULT_REPEAT_PENALTY,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

/// Validated structured output returned by the inference server (`{ mood, text }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanterOutput {
    /// Model-selected discrete persona mood (`牌友情绪`).
    pub mood: PersonaMood,
    /// Single-line Chinese banter text (`牌桌台词`).
    pub text: String,
}

/// Structured request for distilling one concise persona grudge note (`<= 30` chars).
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryNoteRequest {
    /// System prompt establishing persona identity and strict `{ "note": "..." }` JSON output.
    pub system_prompt: String,
    /// User prompt describing the memorable hand or finished match outcome.
    pub user_prompt: String,
    /// Sampling temperature.
    pub temperature: f32,
    /// Nucleus sampling `top_p`.
    pub top_p: f32,
    /// Maximum completion tokens for a concise single-line JSON output.
    pub max_tokens: u32,
}

impl MemoryNoteRequest {
    /// Builds a memory note generation request using tuned concise-output defaults.
    #[must_use]
    pub fn new(system_prompt: impl Into<String>, user_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            user_prompt: user_prompt.into(),
            temperature: 0.7,
            top_p: DEFAULT_TOP_P,
            max_tokens: 80,
        }
    }
}

/// Successful health/model probe result returned by [`LlmClient::health_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmHealthStatus {
    /// Normalized endpoint base URL.
    pub endpoint: String,
    /// Discovered active model identifier from `/v1/models`.
    pub model_id: String,
}

/// Errors produced by [`LlmClient`] probing or completion calls.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LlmError {
    /// Configured base URL is malformed or uses an unsupported scheme.
    #[error("推理服务地址无效（{endpoint}）：{reason}")]
    InvalidEndpoint { endpoint: String, reason: String },
    /// TCP connection or HTTP transport error.
    #[error("无法连接到推理服务（{endpoint}）：{reason}")]
    Transport { endpoint: String, reason: String },
    /// Request exceeded the configured timeout.
    #[error("推理服务请求超时（{endpoint}，超过 {timeout_ms}ms）")]
    Timeout { endpoint: String, timeout_ms: u64 },
    /// HTTP status code was not 2xx.
    #[error("推理服务返回异常状态码 HTTP {status}（{endpoint}）：{body}")]
    HttpStatus {
        endpoint: String,
        status: u16,
        body: String,
    },
    /// Response JSON was missing expected fields or violated schema.
    #[error("推理服务返回格式异常：{reason}")]
    InvalidResponse { reason: String },
    /// `/v1/models` returned an empty model list.
    #[error("推理服务（{endpoint}）未加载可用模型")]
    NoModelsAvailable { endpoint: String },
    /// Response leaked chain-of-thought tags or `reasoning_content`.
    #[error("推理响应包含被禁止的思考链内容")]
    ThinkingLeakage,
    /// Parsed banter text was empty after normalization.
    #[error("推理响应台词文本为空")]
    EmptyBanterText,
}

/// Narrow asynchronous boundary for talking to the LAN inference service.
///
/// Production code uses [`OpenAiLlmClient`]; tests inject local fake HTTP servers
/// or deterministic in-memory stubs through `ServerConfig::with_llm_client` or
/// `TableOptions::with_llm_client`.
pub trait LlmClient: Send + Sync + std::fmt::Debug {
    /// Returns the human-readable endpoint description for diagnostics and errors.
    fn endpoint(&self) -> &str;

    /// Probes `/health` and `/v1/models` to verify the inference service is ready.
    fn health_check(&self) -> LlmFuture<'_, Result<LlmHealthStatus, LlmError>>;

    /// Requests one structured `{ mood, text }` banter output from the inference service.
    fn generate_banter(
        &self,
        request: BanterGenerationRequest,
    ) -> LlmFuture<'_, Result<BanterOutput, LlmError>>;

    /// Requests one concise structured `{ note }` grudge note (`<= 30` chars) for `PersonaMemory`.
    ///
    /// Defaults to returning an empty string (ignored by storage) so existing test doubles
    /// compile without changes while production and memory tests override it.
    fn generate_memory_note(
        &self,
        _request: MemoryNoteRequest,
    ) -> LlmFuture<'_, Result<String, LlmError>> {
        Box::pin(async { Ok(String::new()) })
    }
}

impl From<LanHttpError> for LlmError {
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

/// Resolves the effective LLM base URL with strict precedence:
/// CLI `--llm-url` / `MAJJJ_LLM_URL` > persisted `preferences.json` `llm_base_url` > [`DEFAULT_LLM_BASE_URL`].
#[must_use]
pub fn resolve_llm_base_url(cli_or_env: Option<&str>, persisted_pref: Option<&str>) -> String {
    resolve_lan_base_url(cli_or_env, persisted_pref, DEFAULT_LLM_BASE_URL)
}

/// Production HTTP/1.1 OpenAI-compatible client targeting `llama-server`.
pub struct OpenAiLlmClient {
    endpoint: ParsedEndpoint,
    health_timeout: Duration,
    request_timeout: Duration,
    discovered_model: RwLock<Option<String>>,
}

impl std::fmt::Debug for OpenAiLlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiLlmClient")
            .field("endpoint", &self.endpoint.raw_normalized)
            .field("health_timeout", &self.health_timeout)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

impl OpenAiLlmClient {
    /// Creates a new client for the given `http://...` base URL with default timeouts.
    ///
    /// # Errors
    ///
    /// Returns [`LlmError::InvalidEndpoint`] if `base_url` is malformed.
    pub fn new(base_url: &str) -> Result<Self, LlmError> {
        let endpoint = ParsedEndpoint::parse(base_url)?;
        Ok(Self {
            endpoint,
            health_timeout: DEFAULT_HEALTH_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            discovered_model: RwLock::new(None),
        })
    }

    /// Overrides the timeout used for `/health` and `/v1/models` probing.
    #[must_use]
    pub fn with_health_timeout(mut self, timeout: Duration) -> Self {
        self.health_timeout = timeout;
        self
    }

    /// Overrides the timeout used for `/v1/chat/completions` generation requests.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    fn current_model(&self) -> String {
        self.discovered_model
            .read()
            .ok()
            .and_then(|guard| guard.clone())
            .unwrap_or_else(|| "local".to_string())
    }

    fn set_discovered_model(&self, model: String) {
        if let Ok(mut guard) = self.discovered_model.write() {
            *guard = Some(model);
        }
    }

    async fn send_http(
        &self,
        method: Method,
        suffix: &str,
        body: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Result<(StatusCode, Vec<u8>), LlmError> {
        let (status, _headers, collected) = self
            .endpoint
            .send_http(method, suffix, body, timeout)
            .await?;
        Ok((status, collected))
    }
}

#[derive(Debug, Deserialize)]
struct HealthBody {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

fn contains_thinking_tag(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("<think>") || lower.contains("</think>")
}

fn extract_completion_content(body: &[u8], context_label: &str) -> Result<String, LlmError> {
    let parsed: ChatCompletionResponse =
        serde_json::from_slice(body).map_err(|err| LlmError::InvalidResponse {
            reason: format!("{context_label} JSON 解析失败：{err}"),
        })?;
    let Some(choice) = parsed.choices.into_iter().next() else {
        return Err(LlmError::InvalidResponse {
            reason: "choices 为空".to_string(),
        });
    };
    if choice
        .message
        .reasoning_content
        .as_deref()
        .is_some_and(|rc| !rc.trim().is_empty())
    {
        return Err(LlmError::ThinkingLeakage);
    }
    let Some(content) = choice.message.content else {
        return Err(LlmError::InvalidResponse {
            reason: "message.content 缺失".to_string(),
        });
    };
    if contains_thinking_tag(&content) {
        return Err(LlmError::ThinkingLeakage);
    }
    Ok(content)
}

fn build_chat_completions_payload(
    model: &str,
    request: &BanterGenerationRequest,
) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": request.system_prompt },
            { "role": "user", "content": request.user_prompt }
        ],
        "temperature": request.temperature,
        "top_p": request.top_p,
        "presence_penalty": request.presence_penalty,
        "repeat_penalty": request.repeat_penalty,
        "max_tokens": request.max_tokens,
        "stream": false,
        "chat_template_kwargs": {
            "enable_thinking": false
        },
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "table_banter",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "mood": {
                            "type": "string",
                            "enum": ["Smug", "Mocking", "Tilted", "Nervous", "Plotting", "Deadpan"]
                        },
                        "text": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": 64
                        }
                    },
                    "required": ["mood", "text"],
                    "additionalProperties": false
                }
            }
        }
    })
}

/// Validates and parses a raw OpenAI `/v1/chat/completions` JSON response body into [`BanterOutput`].
fn parse_chat_completion_response(body: &[u8]) -> Result<BanterOutput, LlmError> {
    let content = extract_completion_content(body, "chat completion")?;
    let output: BanterOutput =
        serde_json::from_str(content.trim()).map_err(|err| LlmError::InvalidResponse {
            reason: format!("结构化 {{ mood, text }} 解析失败：{err}"),
        })?;
    if contains_thinking_tag(&output.text) {
        return Err(LlmError::ThinkingLeakage);
    }
    let text = normalize_single_line_text(&output.text);
    if text.is_empty() {
        return Err(LlmError::EmptyBanterText);
    }
    Ok(BanterOutput {
        mood: output.mood,
        text,
    })
}

#[derive(Debug, Deserialize)]
struct MemoryNoteJsonOutput {
    note: String,
}

fn build_memory_note_payload(model: &str, request: &MemoryNoteRequest) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": request.system_prompt },
            { "role": "user", "content": request.user_prompt }
        ],
        "temperature": request.temperature,
        "top_p": request.top_p,
        "max_tokens": request.max_tokens,
        "stream": false,
        "chat_template_kwargs": {
            "enable_thinking": false
        },
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "persona_memory_note",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "note": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": 48
                        }
                    },
                    "required": ["note"],
                    "additionalProperties": false
                }
            }
        }
    })
}

fn parse_memory_note_response(body: &[u8]) -> Result<String, LlmError> {
    let content = extract_completion_content(body, "memory note chat completion")?;
    let output: MemoryNoteJsonOutput =
        serde_json::from_str(content.trim()).map_err(|err| LlmError::InvalidResponse {
            reason: format!("结构化 {{ note }} 解析失败：{err}"),
        })?;
    if contains_thinking_tag(&output.note) {
        return Err(LlmError::ThinkingLeakage);
    }
    let note = normalize_grudge_note(&output.note);
    if note.is_empty() {
        return Err(LlmError::EmptyBanterText);
    }
    Ok(note)
}

impl OpenAiLlmClient {
    fn ensure_http_success(&self, status: StatusCode, body: &[u8]) -> Result<(), LlmError> {
        if status.is_success() {
            return Ok(());
        }
        Err(LlmError::HttpStatus {
            endpoint: self.endpoint.raw_normalized.clone(),
            status: status.as_u16(),
            body: format_http_error_snippet(body),
        })
    }

    async fn post_chat_completion(
        &self,
        payload: &serde_json::Value,
        serialize_label: &str,
    ) -> Result<Vec<u8>, LlmError> {
        let bytes = serde_json::to_vec(payload).map_err(|err| LlmError::InvalidResponse {
            reason: format!("{serialize_label}：{err}"),
        })?;
        let (status, body) = self
            .send_http(
                Method::POST,
                "/v1/chat/completions",
                Some(bytes),
                self.request_timeout,
            )
            .await?;
        self.ensure_http_success(status, &body)?;
        Ok(body)
    }
}

impl LlmClient for OpenAiLlmClient {
    fn endpoint(&self) -> &str {
        &self.endpoint.raw_normalized
    }

    fn health_check(&self) -> LlmFuture<'_, Result<LlmHealthStatus, LlmError>> {
        Box::pin(async move {
            let (health_status, health_body) = self
                .send_http(Method::GET, "/health", None, self.health_timeout)
                .await?;
            self.ensure_http_success(health_status, &health_body)?;
            if let Ok(parsed_health) = serde_json::from_slice::<HealthBody>(&health_body) {
                if let Some(status_text) = parsed_health.status {
                    if !status_text.eq_ignore_ascii_case("ok") {
                        return Err(LlmError::InvalidResponse {
                            reason: format!("/health 状态非就绪：{status_text}"),
                        });
                    }
                }
            }

            let (models_status, models_body) = self
                .send_http(Method::GET, "/v1/models", None, self.health_timeout)
                .await?;
            self.ensure_http_success(models_status, &models_body)?;
            let models: ModelsResponse =
                serde_json::from_slice(&models_body).map_err(|err| LlmError::InvalidResponse {
                    reason: format!("/v1/models JSON 解析失败：{err}"),
                })?;
            let Some(model_id) = models
                .data
                .into_iter()
                .map(|m| m.id.trim().to_string())
                .find(|id| !id.is_empty())
            else {
                return Err(LlmError::NoModelsAvailable {
                    endpoint: self.endpoint.raw_normalized.clone(),
                });
            };

            self.set_discovered_model(model_id.clone());
            Ok(LlmHealthStatus {
                endpoint: self.endpoint.raw_normalized.clone(),
                model_id,
            })
        })
    }

    fn generate_banter(
        &self,
        request: BanterGenerationRequest,
    ) -> LlmFuture<'_, Result<BanterOutput, LlmError>> {
        Box::pin(async move {
            let model = self.current_model();
            let payload = build_chat_completions_payload(&model, &request);
            let body = self
                .post_chat_completion(&payload, "序列化请求失败")
                .await?;
            parse_chat_completion_response(&body)
        })
    }

    fn generate_memory_note(
        &self,
        request: MemoryNoteRequest,
    ) -> LlmFuture<'_, Result<String, LlmError>> {
        Box::pin(async move {
            let model = self.current_model();
            let payload = build_memory_note_payload(&model, &request);
            let body = self
                .post_chat_completion(&payload, "序列化记忆笔记请求失败")
                .await?;
            parse_memory_note_response(&body)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_chat_completions_payload, build_memory_note_payload, parse_chat_completion_response,
        parse_memory_note_response, resolve_llm_base_url, BanterGenerationRequest, BanterOutput,
        LlmError, MemoryNoteRequest, OpenAiLlmClient, DEFAULT_LLM_BASE_URL,
    };
    use crate::persona::PersonaMood;

    #[test]
    fn resolve_llm_base_url_follows_cli_over_prefs_over_default() {
        assert_eq!(resolve_llm_base_url(None, None), DEFAULT_LLM_BASE_URL);
        assert_eq!(
            resolve_llm_base_url(Some("  "), Some("")),
            DEFAULT_LLM_BASE_URL
        );
        assert_eq!(
            resolve_llm_base_url(None, Some("http://192.168.5.99:8080")),
            "http://192.168.5.99:8080"
        );
        assert_eq!(
            resolve_llm_base_url(
                Some("http://127.0.0.1:18080"),
                Some("http://192.168.5.99:8080")
            ),
            "http://127.0.0.1:18080"
        );
    }

    #[test]
    fn openai_client_rejects_malformed_or_non_http_endpoints() {
        assert!(matches!(
            OpenAiLlmClient::new("https://192.168.5.50:8080"),
            Err(LlmError::InvalidEndpoint { .. })
        ));
        assert!(matches!(
            OpenAiLlmClient::new("192.168.5.50:8080"),
            Err(LlmError::InvalidEndpoint { .. })
        ));
        assert!(matches!(
            OpenAiLlmClient::new("http://"),
            Err(LlmError::InvalidEndpoint { .. })
        ));
        assert!(matches!(
            OpenAiLlmClient::new("http://192.168.5.50:notaport"),
            Err(LlmError::InvalidEndpoint { .. })
        ));

        let ok = OpenAiLlmClient::new("http://192.168.5.50:8080/").expect("valid URL");
        assert_eq!(ok.endpoint.raw_normalized, "http://192.168.5.50:8080");
        let ipv6 = OpenAiLlmClient::new("http://[::1]:8080/proxy/").expect("valid IPv6 URL");
        assert_eq!(ipv6.endpoint.raw_normalized, "http://[::1]:8080/proxy");
        assert_eq!(ipv6.endpoint.path_for("/health"), "/proxy/health");
    }

    #[test]
    fn request_payload_enforces_no_thinking_json_schema_and_sampling_params() {
        let req = BanterGenerationRequest::new("system rules", "user event");
        let json = build_chat_completions_payload("Qwen3.5-9B-Q5_K_M", &req);

        assert_eq!(json["model"], "Qwen3.5-9B-Q5_K_M");
        assert_eq!(json["stream"], false);
        assert_eq!(json["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(json["response_format"]["type"], "json_schema");
        assert_eq!(json["response_format"]["json_schema"]["strict"], true);
        let moods = json["response_format"]["json_schema"]["schema"]["properties"]["mood"]["enum"]
            .as_array()
            .expect("mood enum array");
        assert_eq!(moods.len(), 6);
        assert!((json["temperature"].as_f64().unwrap() - 0.9).abs() < 1e-4);
        assert!((json["top_p"].as_f64().unwrap() - 0.9).abs() < 1e-4);
        assert!((json["presence_penalty"].as_f64().unwrap() - 0.4).abs() < 1e-4);
    }

    #[test]
    fn response_parser_accepts_valid_json_and_rejects_thinking_leakage_or_empty_text() {
        let valid = r#"{"choices":[{"message":{"content":"{\"mood\":\"Smug\",\"text\":\"  这把速攻直接收菜！\\n还想拦我？ \"}"}}]}"#.as_bytes();
        let parsed = parse_chat_completion_response(valid).expect("valid response");
        assert_eq!(
            parsed,
            BanterOutput {
                mood: PersonaMood::Smug,
                text: "这把速攻直接收菜！ 还想拦我？".to_string(),
            }
        );

        let think_tag = r#"{"choices":[{"message":{"content":"<think>hmm</think>{\"mood\":\"Smug\",\"text\":\"你好\"}"}}]}"#.as_bytes();
        assert_eq!(
            parse_chat_completion_response(think_tag),
            Err(LlmError::ThinkingLeakage)
        );

        let think_inside_text = r#"{"choices":[{"message":{"content":"{\"mood\":\"Tilted\",\"text\":\"<think>oops</think>破防了\"}"}}]}"#.as_bytes();
        assert_eq!(
            parse_chat_completion_response(think_inside_text),
            Err(LlmError::ThinkingLeakage)
        );

        let reasoning_field = r#"{"choices":[{"message":{"content":"{\"mood\":\"Mocking\",\"text\":\"呵呵\"}","reasoning_content":"secret thoughts"}}]}"#.as_bytes();
        assert_eq!(
            parse_chat_completion_response(reasoning_field),
            Err(LlmError::ThinkingLeakage)
        );

        let empty_text = r#"{"choices":[{"message":{"content":"{\"mood\":\"Deadpan\",\"text\":\"   \\n  \"}"}}]}"#.as_bytes();
        assert_eq!(
            parse_chat_completion_response(empty_text),
            Err(LlmError::EmptyBanterText)
        );

        let invalid_mood =
            r#"{"choices":[{"message":{"content":"{\"mood\":\"Furious\",\"text\":\"气死了\"}"}}]}"#
                .as_bytes();
        assert!(matches!(
            parse_chat_completion_response(invalid_mood),
            Err(LlmError::InvalidResponse { .. })
        ));
    }

    #[test]
    fn memory_note_payload_and_parser_disable_thinking_and_clamp_to_thirty_chars() {
        let req = MemoryNoteRequest::new("system note", "user note");
        let payload = build_memory_note_payload("Qwen3.5-9B-Q5_K_M", &req);
        assert_eq!(payload["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(payload["response_format"]["type"], "json_schema");
        assert_eq!(
            payload["response_format"]["json_schema"]["name"],
            "persona_memory_note"
        );

        let long_raw = r#"{"choices":[{"message":{"content":"{\"note\":\"  小王上回清一色自摸把我打得怀疑人生，下次见他必须先扣住他的万子绝不留情面  \"}"}}]}"#
            .as_bytes();
        let parsed = parse_memory_note_response(long_raw).expect("valid memory note");
        assert_eq!(parsed.chars().count(), 30);

        let thinking =
            r#"{"choices":[{"message":{"content":"<think>x</think>{\"note\":\"记住小王\"}"}}]}"#
                .as_bytes();
        assert_eq!(
            parse_memory_note_response(thinking),
            Err(LlmError::ThinkingLeakage)
        );
    }
}
