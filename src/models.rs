use serde::Deserialize;

// Response types for API calls
#[derive(Debug, Deserialize)]
pub struct Screen {
    pub name: Option<String>,
    #[serde(rename = "screenId")]
    pub screen_id: String,
    #[serde(rename = "loungeToken")]
    pub lounge_token: String,
}

#[derive(Debug, Deserialize)]
pub struct ScreenResponse {
    pub screen: Screen,
}

#[derive(Debug, Deserialize)]
pub struct ScreensResponse {
    pub screens: Vec<Screen>,
}

#[derive(Debug, Deserialize)]
pub struct ScreenAvailability {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ScreenAvailabilityResponse {
    pub screens: Vec<ScreenAvailability>,
}

// YouTube video data
#[derive(Debug, Clone, Deserialize)]
pub struct VideoData {
    pub video_id: String,
    pub author: String,
    pub title: String,
    #[serde(default)]
    pub is_playable: bool,
}

// Playback state event
/// Enum representing different playback states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStateValue {
    /// Video is currently playing
    Playing = 1,
    /// Video is paused
    Paused = 2,
    /// Video is buffering
    Buffering = 3,
    /// Video has ended
    Ended = 5,
    /// Unknown state
    Unknown = 0,
}

impl PlaybackStateValue {
    /// Convert from integer state to enum value
    pub fn from_i32(state: i32) -> Self {
        match state {
            1 => PlaybackStateValue::Playing,
            2 => PlaybackStateValue::Paused,
            3 => PlaybackStateValue::Buffering,
            5 => PlaybackStateValue::Ended,
            _ => PlaybackStateValue::Unknown,
        }
    }

    /// Get string representation of the state
    pub fn as_str(&self) -> &'static str {
        match self {
            PlaybackStateValue::Playing => "Playing",
            PlaybackStateValue::Paused => "Paused",
            PlaybackStateValue::Buffering => "Buffering",
            PlaybackStateValue::Ended => "Ended",
            PlaybackStateValue::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaybackState {
    #[serde(rename = "currentTime")]
    pub current_time: f64,
    #[serde(rename = "videoId")]
    pub video_id: String,
    pub state: i32,
    pub duration: f64,
    #[serde(rename = "seekableStartTime")]
    pub seekable_start_time: f64,
    #[serde(rename = "seekableEndTime")]
    pub seekable_end_time: f64,
    pub volume: i32,
    pub muted: bool,
    #[serde(rename = "videoData")]
    pub video_data: VideoData,
}

impl PlaybackState {
    /// Get the playback state as a strongly-typed enum value
    pub fn state_value(&self) -> PlaybackStateValue {
        PlaybackStateValue::from_i32(self.state)
    }

    /// Returns true if the video is currently playing
    pub fn is_playing(&self) -> bool {
        self.state_value() == PlaybackStateValue::Playing
    }

    /// Returns true if the video is currently paused
    pub fn is_paused(&self) -> bool {
        self.state_value() == PlaybackStateValue::Paused
    }

    /// Returns the state as a human-readable string
    pub fn state_name(&self) -> &'static str {
        self.state_value().as_str()
    }
}

// Now playing event
#[derive(Debug, Clone, Deserialize)]
pub struct NowPlaying {
    #[serde(rename = "videoId")]
    pub video_id: String,
    #[serde(rename = "currentTime")]
    pub current_time: f64,
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    pub state: i32,
    #[serde(rename = "videoData")]
    pub video_data: VideoData,
}

// Device info
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceInfo {
    pub brand: String,
    pub model: String,
    #[serde(rename = "deviceType")]
    pub device_type: String,
}

// Device
#[derive(Debug, Clone, Deserialize)]
pub struct Device {
    pub app: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: String,
    #[serde(rename = "deviceInfo")]
    pub device_info_raw: String,
    #[serde(skip)]
    pub device_info: Option<DeviceInfo>,
}

// Lounge status event
#[derive(Debug, Clone, Deserialize)]
pub struct LoungeStatus {
    pub devices: String,
}

// Ad state change event
#[derive(Debug, Clone, Deserialize)]
pub struct AdState {
    #[serde(rename = "contentVideoId")]
    pub content_video_id: String,
    #[serde(rename = "isSkipEnabled")]
    pub is_skip_enabled: bool,
}

// Subtitles track changed event
#[derive(Debug, Clone, Deserialize)]
pub struct SubtitlesTrackChanged {
    #[serde(rename = "videoId")]
    pub video_id: String,
}

// Autoplay mode changed event
#[derive(Debug, Clone, Deserialize)]
pub struct AutoplayModeChanged {
    #[serde(rename = "autoplayMode")]
    pub autoplay_mode: String,
}

// Has previous/next changed event
#[derive(Debug, Clone, Deserialize)]
pub struct HasPreviousNextChanged {
    #[serde(rename = "hasNext")]
    pub has_next: String,
    #[serde(rename = "hasPrevious")]
    pub has_previous: String,
}

// Video quality changed event
#[derive(Debug, Clone, Deserialize)]
pub struct VideoQualityChanged {
    #[serde(rename = "availableQualityLevels")]
    pub available_quality_levels: String,
    #[serde(rename = "qualityLevel")]
    pub quality_level: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
}

// Audio track changed event
#[derive(Debug, Clone, Deserialize)]
pub struct AudioTrackChanged {
    #[serde(rename = "audioTrackId")]
    pub audio_track_id: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
}

// Playlist modified event
#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistModified {
    #[serde(rename = "currentIndex")]
    pub current_index: String,
    #[serde(rename = "firstVideoId")]
    pub first_video_id: String,
    #[serde(rename = "listId")]
    pub list_id: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
}
