//! LAN mahjong table server crate.

pub use mahjong_ai;
pub use mahjong_core;

pub mod ai_pool;
pub mod banter;
pub mod banter_audio;
pub mod history_dto;
pub mod lan;
mod lan_http;
pub mod lifecycle;
pub mod llm;
pub mod pacing;
pub mod persona;
pub mod persona_memory;
pub mod protocol;
mod seat_control;
pub mod server;
pub mod session;
pub mod static_assets;
pub mod storage;
pub mod tts;

pub use ai_pool::default_ai_threads;
pub use banter_audio::{
    BanterAudioCache, BANTER_AUDIO_CACHE_MAX_BYTES, BANTER_AUDIO_CACHE_MAX_ENTRIES,
    BANTER_AUDIO_CACHE_TTL, TTS_FAILURE_THRESHOLD,
};
pub use lifecycle::IdleTimeouts;
pub use llm::{
    BanterGenerationRequest, BanterOutput, LlmClient, LlmError, LlmHealthStatus, MemoryNoteRequest,
    OpenAiLlmClient, DEFAULT_LLM_BASE_URL,
};
pub use pacing::PacingMode;
pub use server::{HeartbeatConfig, ServerConfig, ServerError, ServerHandle};
pub use session::{BoxedWallProvider, TableOptions, WallProviderFactory};
pub use tts::{
    resolve_tts_base_url, HttpTtsClient, TtsAudioOutput, TtsClient, TtsError, TtsFuture,
    TtsHealthStatus, TtsSynthesisRequest, DEFAULT_TTS_BASE_URL, TTS_COOLDOWN_DURATION,
    TTS_IN_HAND_DEADLINE_MS, TTS_SETTLEMENT_DEADLINE_MS,
};
