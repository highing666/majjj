//! LAN mahjong table server crate.

pub use mahjong_ai;
pub use mahjong_core;

pub mod ai_pool;
pub mod banter;
pub mod banter_audio;
pub mod banter_context;
pub mod history_dto;
pub mod lan;
mod lan_http;
pub mod lifecycle;
pub mod llm;
mod net_bind;
pub mod pacing;
pub mod persona;
pub mod persona_memory;
pub mod protocol;
pub mod public_url;
pub mod remote_access;
mod remote_control;
mod remote_gate;
mod seat_control;
pub mod security;
pub mod server;
pub mod session;
pub mod static_assets;
pub mod storage;
pub mod tts;

pub use ai_pool::default_ai_threads;
pub use banter_audio::{
    ActiveBanterStream, BanterAudioCache, BanterAudioSource, BANTER_AUDIO_CACHE_MAX_BYTES,
    BANTER_AUDIO_CACHE_MAX_ENTRIES, BANTER_AUDIO_CACHE_TTL, TTS_FAILURE_THRESHOLD,
};
pub use lifecycle::IdleTimeouts;
pub use llm::{
    BanterGenerationRequest, BanterOutput, LlmClient, LlmError, LlmHealthStatus, MemoryNoteRequest,
    OpenAiLlmClient, DEFAULT_LLM_BASE_URL,
};
pub use pacing::PacingMode;
pub use remote_access::{
    RemoteAccessManager, RemoteAccessRuntimeStatus, RemoteAuthOutcome, REMOTE_SESSION_COOKIE_NAME,
};
pub use security::ClientTrustTier;
pub use server::{HeartbeatConfig, ServerConfig, ServerError, ServerHandle, TestRouterHandle};
pub use session::{BoxedWallProvider, TableOptions, WallProviderFactory};
pub use tts::{
    resolve_tts_base_url, HttpTtsClient, TtsAudioOutput, TtsAudioStream, TtsChunkStream, TtsClient,
    TtsError, TtsFuture, TtsHealthStatus, TtsSynthesisRequest, DEFAULT_TTS_BASE_URL,
    DEFAULT_TTS_CHANNELS, DEFAULT_TTS_SAMPLE_RATE, TTS_COOLDOWN_DURATION, TTS_IN_HAND_DEADLINE_MS,
    TTS_SETTLEMENT_DEADLINE_MS,
};
