use crate::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, Device,
    HasPreviousNextChanged, NowPlaying, PlaybackState, PlaylistModified, SubtitlesTrackChanged,
    VideoQualityChanged, VolumeChanged,
};

#[derive(Debug, Clone)]
pub enum LoungeEvent {
    StateChange(PlaybackState),
    NowPlaying(NowPlaying),
    LoungeStatus(Vec<Device>, Option<String>),
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
    pub fn name(&self) -> &'static str {
        match self {
            Self::StateChange(_) => "StateChange",
            Self::NowPlaying(_) => "NowPlaying",
            Self::LoungeStatus(_, _) => "LoungeStatus",
            Self::ScreenDisconnected => "ScreenDisconnected",
            Self::SessionEstablished => "SessionEstablished",
            Self::AdStateChange(_) => "AdStateChange",
            Self::SubtitlesTrackChanged(_) => "SubtitlesTrackChanged",
            Self::AutoplayModeChanged(_) => "AutoplayModeChanged",
            Self::HasPreviousNextChanged(_) => "HasPreviousNextChanged",
            Self::VideoQualityChanged(_) => "VideoQualityChanged",
            Self::AudioTrackChanged(_) => "AudioTrackChanged",
            Self::PlaylistModified(_) => "PlaylistModified",
            Self::AutoplayUpNext(_) => "AutoplayUpNext",
            Self::VolumeChanged(_) => "VolumeChanged",
            Self::Unknown(_) => "Unknown",
        }
    }

    pub fn event_type(&self) -> &'static str {
        match self {
            Self::StateChange(_) => "onStateChange",
            Self::NowPlaying(_) => "nowPlaying",
            Self::LoungeStatus(_, _) => "loungeStatus",
            Self::ScreenDisconnected => "loungeScreenDisconnected",
            Self::SessionEstablished => "sessionEstablished",
            Self::AdStateChange(_) => "onAdStateChange",
            Self::SubtitlesTrackChanged(_) => "onSubtitlesTrackChanged",
            Self::AutoplayModeChanged(_) => "onAutoplayModeChanged",
            Self::HasPreviousNextChanged(_) => "onHasPreviousNextChanged",
            Self::VideoQualityChanged(_) => "onVideoQualityChanged",
            Self::AudioTrackChanged(_) => "onAudioTrackChanged",
            Self::PlaylistModified(_) => "playlistModified",
            Self::AutoplayUpNext(_) => "autoplayUpNext",
            Self::VolumeChanged(_) => "onVolumeChanged",
            Self::Unknown(_) => "unknown",
        }
    }

    pub fn is_showing_ad(&self) -> bool {
        matches!(self, Self::AdStateChange(_))
    }
    pub fn ad_state(&self) -> Option<&AdState> {
        if let Self::AdStateChange(s) = self {
            Some(s)
        } else {
            None
        }
    }
}
