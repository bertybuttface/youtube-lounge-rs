use crate::models::{AdState, Device, NowPlaying, PlaybackState};

// Event types for the callback
#[derive(Debug, Clone)]
pub enum LoungeEvent {
    StateChange(PlaybackState),
    NowPlaying(NowPlaying),
    LoungeStatus(Vec<Device>),
    ScreenDisconnected,
    SessionEstablished,
    AdStateChange(AdState),
    Unknown(String),
}
