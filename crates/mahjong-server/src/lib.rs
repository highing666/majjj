//! LAN mahjong table server crate.

pub use mahjong_ai;
pub use mahjong_core;

pub mod ai_pool;
pub mod banter;
pub mod history_dto;
pub mod lan;
pub mod lifecycle;
pub mod llm;
pub mod pacing;
pub mod persona;
pub mod protocol;
mod seat_control;
pub mod server;
pub mod session;
pub mod static_assets;
pub mod storage;

pub use ai_pool::default_ai_threads;
pub use lifecycle::IdleTimeouts;
pub use llm::{
    BanterGenerationRequest, BanterOutput, LlmClient, LlmError, LlmHealthStatus, OpenAiLlmClient,
    DEFAULT_LLM_BASE_URL,
};
pub use pacing::PacingMode;
pub use server::{HeartbeatConfig, ServerConfig, ServerError, ServerHandle};
pub use session::{BoxedWallProvider, TableOptions, WallProviderFactory};
