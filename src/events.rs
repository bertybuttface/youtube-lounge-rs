use crate::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, Device, HasPreviousNextChanged, NowPlaying,
    PlaybackState, PlaylistModified, SubtitlesTrackChanged, VideoQualityChanged,
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
    AudioTrackChanged(AudioTrackChanged),
    PlaylistModified(PlaylistModified),
    Unknown(String),
}
