// Re-export all public items
pub mod client;
pub mod commands;
pub mod error;
pub mod events;
pub mod models;
pub mod session;
pub mod utils;

// Re-export common items for easier use
pub use client::LoungeClient;
pub use commands::PlaybackCommand;
pub use error::LoungeError;
pub use events::LoungeEvent;
pub use models::{Device, DeviceInfo, NowPlaying, PlaybackState, PlaybackStateValue, Screen};
pub use session::PlaybackSessionManager;
pub use utils::parsing::{HasDuration, HasVolume, YoutubeValueParser};
pub use utils::state::{HasPlaybackState, PlaybackStatus};
