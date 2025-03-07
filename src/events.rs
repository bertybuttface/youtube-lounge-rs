use crate::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, Device,
    HasPreviousNextChanged, NowPlaying, PlaybackState, PlaylistModified, SubtitlesTrackChanged,
    VideoQualityChanged,
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
    AutoplayUpNext(AutoplayUpNext),
    Unknown(String),
}

impl LoungeEvent {
    // Get the name of the event type
    pub fn event_type(&self) -> &'static str {
        match self {
            LoungeEvent::StateChange(_) => "onStateChange",
            LoungeEvent::NowPlaying(_) => "nowPlaying",
            LoungeEvent::LoungeStatus(_) => "loungeStatus",
            LoungeEvent::ScreenDisconnected => "loungeScreenDisconnected",
            LoungeEvent::SessionEstablished => "sessionEstablished",
            LoungeEvent::AdStateChange(_) => "onAdStateChange",
            LoungeEvent::SubtitlesTrackChanged(_) => "onSubtitlesTrackChanged",
            LoungeEvent::AutoplayModeChanged(_) => "onAutoplayModeChanged",
            LoungeEvent::HasPreviousNextChanged(_) => "onHasPreviousNextChanged",
            LoungeEvent::VideoQualityChanged(_) => "onVideoQualityChanged",
            LoungeEvent::AudioTrackChanged(_) => "onAudioTrackChanged",
            LoungeEvent::PlaylistModified(_) => "playlistModified",
            LoungeEvent::AutoplayUpNext(_) => "autoplayUpNext",
            LoungeEvent::Unknown(event_type) => {
                // For Unknown events, we manually extract the event type from the string
                // and return a static string to avoid lifetime issues
                if event_type.contains("onAdStateChange") {
                    "onAdStateChange"
                } else if event_type.contains("onSubtitlesTrackChanged") {
                    "onSubtitlesTrackChanged"
                } else if event_type.contains("onAutoplayModeChanged") {
                    "onAutoplayModeChanged"
                } else if event_type.contains("onHasPreviousNextChanged") {
                    "onHasPreviousNextChanged"
                } else if event_type.contains("onVideoQualityChanged") {
                    "onVideoQualityChanged"
                } else if event_type.contains("onAudioTrackChanged") {
                    "onAudioTrackChanged"
                } else if event_type.contains("playlistModified") {
                    "playlistModified"
                } else if event_type.contains("autoplayUpNext") {
                    "autoplayUpNext"
                } else {
                    "unknown"
                }
            }
        }
    }

    /// Returns true if an ad is currently being shown
    ///
    /// This is determined by checking if the event is an AdStateChange event,
    /// which indicates that an ad is currently being displayed
    pub fn is_showing_ad(&self) -> bool {
        matches!(self, LoungeEvent::AdStateChange(_))
    }

    /// If this event is an AdStateChange, returns the AdState
    /// Otherwise returns None
    pub fn ad_state(&self) -> Option<&AdState> {
        match self {
            LoungeEvent::AdStateChange(state) => Some(state),
            _ => None,
        }
    }
}
