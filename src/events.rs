use crate::models::{
    AdState, AutoplayModeChanged, Device, HasPreviousNextChanged, NowPlaying, PlaybackState,
    SubtitlesTrackChanged, VideoQualityChanged,
};

// Event types for the callback
#[derive(Debug, Clone)]
pub enum LoungeEvent {
    StateChange(PlaybackState),
    NowPlaying(NowPlaying),
    LoungeStatus(Vec<Device>),
    ScreenDisconnected,
    SessionEstablished,
    AdStateChange(AdState),
    SubtitlesTrackChanged(SubtitlesTrackChanged),
    AutoplayModeChanged(AutoplayModeChanged),
    HasPreviousNextChanged(HasPreviousNextChanged),
    VideoQualityChanged(VideoQualityChanged),
    Unknown(String),
}
