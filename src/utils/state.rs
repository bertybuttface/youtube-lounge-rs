/// Unified representation of YouTube playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    /// Video is stopped (-1)
    Stopped,
    /// Buffering between videos (0)
    BufferingBetween,
    /// Video is currently playing (1)
    Playing,
    /// Video is paused (2)
    Paused,
    /// Video is starting (3)
    Starting,
    /// Video has ended (5)
    Ended,
    /// Advertisement is playing (1081)
    Advertisement,
    /// Unknown state
    Unknown,
}

impl PlaybackStatus {
    /// Convert from integer state to enum value
    pub fn from_i32(state: i32) -> Self {
        match state {
            -1 => PlaybackStatus::Stopped,
            0 => PlaybackStatus::BufferingBetween,
            1 => PlaybackStatus::Playing,
            2 => PlaybackStatus::Paused,
            3 => PlaybackStatus::Starting,
            5 => PlaybackStatus::Ended,
            1081 => PlaybackStatus::Advertisement,
            _ => PlaybackStatus::Unknown,
        }
    }

    /// Convert to integer representation
    pub fn to_i32(self) -> i32 {
        match self {
            PlaybackStatus::Stopped => -1,
            PlaybackStatus::BufferingBetween => 0,
            PlaybackStatus::Playing => 1,
            PlaybackStatus::Paused => 2,
            PlaybackStatus::Starting => 3,
            PlaybackStatus::Ended => 5,
            PlaybackStatus::Advertisement => 1081,
            PlaybackStatus::Unknown => -99, // Special value for unknown state
        }
    }

    /// Get string representation of the state
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackStatus::Stopped => "STOPPED",
            PlaybackStatus::BufferingBetween => "BUFFERING_BETWEEN",
            PlaybackStatus::Playing => "PLAYING",
            PlaybackStatus::Paused => "PAUSED",
            PlaybackStatus::Starting => "STARTING",
            PlaybackStatus::Ended => "ENDED",
            PlaybackStatus::Advertisement => "ADVERTISEMENT",
            PlaybackStatus::Unknown => "UNKNOWN",
        }
    }
}

/// Trait for types that have a playback state
pub trait HasPlaybackState {
    /// Get the current playback state
    fn status(&self) -> PlaybackStatus;

    /// Check if media is currently playing
    fn is_playing(&self) -> bool {
        self.status() == PlaybackStatus::Playing
    }

    /// Check if media is currently paused
    fn is_paused(&self) -> bool {
        self.status() == PlaybackStatus::Paused
    }

    /// Check if media is currently buffering
    fn is_buffering(&self) -> bool {
        self.status() == PlaybackStatus::BufferingBetween
    }

    /// Check if media is currently stopped
    fn is_stopped(&self) -> bool {
        self.status() == PlaybackStatus::Stopped
    }

    /// Check if an advertisement is playing
    fn is_ad(&self) -> bool {
        self.status() == PlaybackStatus::Advertisement
    }
}
