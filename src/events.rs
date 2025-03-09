use crate::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, Device,
    HasPreviousNextChanged, NowPlaying, PlaybackState, PlaylistModified, SubtitlesTrackChanged,
    VideoQualityChanged, VolumeChanged,
};

// Event types for the callback
#[derive(Debug, Clone)]
pub enum LoungeEvent {
    StateChange(PlaybackState),
    NowPlaying(NowPlaying),
    LoungeStatus(Vec<Device>, Option<String>), // Add queue_id parameter
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
    VolumeChanged(VolumeChanged),
    Unknown(String),
}

impl LoungeEvent {
    // Get the name of the event type for logging purposes
    pub fn name(&self) -> &'static str {
        match self {
            LoungeEvent::StateChange(_) => "StateChange",
            LoungeEvent::NowPlaying(_) => "NowPlaying",
            LoungeEvent::LoungeStatus(_, _) => "LoungeStatus",
            LoungeEvent::ScreenDisconnected => "ScreenDisconnected",
            LoungeEvent::SessionEstablished => "SessionEstablished",
            LoungeEvent::AdStateChange(_) => "AdStateChange",
            LoungeEvent::SubtitlesTrackChanged(_) => "SubtitlesTrackChanged",
            LoungeEvent::AutoplayModeChanged(_) => "AutoplayModeChanged",
            LoungeEvent::HasPreviousNextChanged(_) => "HasPreviousNextChanged",
            LoungeEvent::VideoQualityChanged(_) => "VideoQualityChanged",
            LoungeEvent::AudioTrackChanged(_) => "AudioTrackChanged",
            LoungeEvent::PlaylistModified(_) => "PlaylistModified",
            LoungeEvent::AutoplayUpNext(_) => "AutoplayUpNext",
            LoungeEvent::VolumeChanged(_) => "VolumeChanged",
            LoungeEvent::Unknown(_) => "Unknown",
        }
    }

    // Get the name of the event type (YouTube API event name)
    pub fn event_type(&self) -> &'static str {
        match self {
            LoungeEvent::StateChange(_) => "onStateChange",
            LoungeEvent::NowPlaying(_) => "nowPlaying",
            LoungeEvent::LoungeStatus(_, _) => "loungeStatus",
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
            LoungeEvent::VolumeChanged(_) => "onVolumeChanged",
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
                } else if event_type.contains("onVolumeChanged") {
                    "onVolumeChanged"
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
