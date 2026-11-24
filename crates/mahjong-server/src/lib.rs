//! LAN mahjong table server crate.

pub use mahjong_ai;
pub use mahjong_core;

pub mod lan;
pub mod protocol;
pub mod server;
pub mod session;
pub mod static_assets;

pub use server::{default_ai_threads, ServerConfig, ServerError, ServerHandle};
pub use session::{BoxedWallProvider, TableOptions, WallProviderFactory};
