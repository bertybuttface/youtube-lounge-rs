use serde::Deserialize;
use std::time::Instant;

// PlaybackSession for tracking video playback across related events
#[derive(Debug, Clone)]
pub struct PlaybackSession {
    pub cpn: String,
    pub video_id: Option<String>,
    pub list_id: Option<String>,
    pub current_time: f64,
    pub duration: f64,
    pub state: i32,
    pub loaded_time: f64,
    pub seekable_start_time: f64,
    pub seekable_end_time: f64,
    pub last_updated: Instant,
    pub video_history: Option<Vec<String>>,
}

impl PlaybackSession {
    // State value constants
    pub const STOPPED: i32 = -1;
    pub const BUFFERING_BETWEEN: i32 = 0;
    pub const PLAYING: i32 = 1;
    pub const PAUSED: i32 = 2;
    pub const STARTING: i32 = 3;
    pub const ADVERTISEMENT: i32 = 1081;

    // Create a new session from a NowPlaying event
    pub fn from_now_playing(event: &NowPlaying) -> Option<Self> {
        let cpn = event.cpn.clone()?;

        Some(Self {
            cpn,
            video_id: Some(event.video_id.clone()),
            list_id: event.list_id.clone(),
            current_time: event.current_time_value(),
            duration: event.duration_value(),
            state: event.state_value(),
            loaded_time: event.loaded_time_value(),
            seekable_start_time: event.seekable_start_time_value(),
            seekable_end_time: event.seekable_end_time_value(),
            last_updated: Instant::now(),
            video_history: event.video_history(),
        })
    }

    // Update from a StateChange event
    pub fn update_from_state_change(&mut self, event: &PlaybackState) -> bool {
        let mut updated = false;

        // Update current time
        let new_current_time = event.current_time_value();
        if self.current_time != new_current_time {
            self.current_time = new_current_time;
            updated = true;
        }

        // Update duration
        let new_duration = event.duration_value();
        if self.duration != new_duration {
            self.duration = new_duration;
            updated = true;
        }

        // Update state
        let new_state = event.state_value();
        if self.state != new_state {
            self.state = new_state;
            updated = true;
        }

        // Update seekable times
        let new_seekable_start = event.seekable_start_time_value();
        if self.seekable_start_time != new_seekable_start {
            self.seekable_start_time = new_seekable_start;
            updated = true;
        }

        let new_seekable_end = event.seekable_end_time_value();
        if self.seekable_end_time != new_seekable_end {
            self.seekable_end_time = new_seekable_end;
            updated = true;
        }

        // Update loaded time
        let new_loaded_time = event.loaded_time_value();
        if self.loaded_time != new_loaded_time {
            self.loaded_time = new_loaded_time;
            updated = true;
        }

        if updated {
            self.last_updated = Instant::now();
        }

        updated
    }

    // Create a basic session from just a state change event
    pub fn from_state_change(event: &PlaybackState) -> Option<Self> {
        let cpn = event.cpn.clone()?;

        Some(Self {
            cpn,
            video_id: Some(event.video_id.clone()),
            list_id: None,
            current_time: event.current_time_value(),
            duration: event.duration_value(),
            state: event.state_value(),
            loaded_time: event.loaded_time_value(),
            seekable_start_time: event.seekable_start_time_value(),
            seekable_end_time: event.seekable_end_time_value(),
            last_updated: Instant::now(),
            video_history: None,
        })
    }

    pub fn state_name(&self) -> &'static str {
        match self.state {
            Self::STOPPED => "STOPPED",
            Self::BUFFERING_BETWEEN => "BUFFERING_BETWEEN",
            Self::PLAYING => "PLAYING",
            Self::PAUSED => "PAUSED",
            Self::STARTING => "STARTING",
            Self::ADVERTISEMENT => "ADVERTISEMENT",
            _ => "UNKNOWN",
        }
    }

    pub fn is_playing(&self) -> bool {
        self.state == Self::PLAYING
    }

    pub fn is_paused(&self) -> bool {
        self.state == Self::PAUSED
    }

    pub fn is_ad(&self) -> bool {
        self.state == Self::ADVERTISEMENT
    }

    pub fn is_buffering(&self) -> bool {
        self.state == Self::BUFFERING_BETWEEN
    }

    pub fn is_stopped(&self) -> bool {
        self.state == Self::STOPPED
    }

    pub fn progress_percentage(&self) -> f64 {
        if self.duration > 0.0 {
            (self.current_time / self.duration) * 100.0
        } else {
            0.0
        }
    }
}

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
#[derive(Debug, Clone, Deserialize, Default)]
pub struct VideoData {
    #[serde(default)]
    pub video_id: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub is_playable: bool,
}

// Playback state event
/// Enum representing different playback states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStateValue {
    /// Video is stopped
    Stopped = -1,
    /// Buffering between videos
    BufferingBetween = 0,
    /// Video is currently playing
    Playing = 1,
    /// Video is paused
    Paused = 2,
    /// Video is starting
    Starting = 3,
    /// Video has ended
    Ended = 5,
    /// Advertisement is playing
    Advertisement = 1081,
    /// Unknown state
    Unknown = -99,
}

impl PlaybackStateValue {
    /// Convert from integer state to enum value
    pub fn from_i32(state: i32) -> Self {
        match state {
            -1 => PlaybackStateValue::Stopped,
            0 => PlaybackStateValue::BufferingBetween,
            1 => PlaybackStateValue::Playing,
            2 => PlaybackStateValue::Paused,
            3 => PlaybackStateValue::Starting,
            5 => PlaybackStateValue::Ended,
            1081 => PlaybackStateValue::Advertisement,
            _ => PlaybackStateValue::Unknown,
        }
    }

    /// Get string representation of the state
    pub fn as_str(&self) -> &'static str {
        match self {
            PlaybackStateValue::Stopped => "STOPPED",
            PlaybackStateValue::BufferingBetween => "BUFFERING_BETWEEN",
            PlaybackStateValue::Playing => "PLAYING",
            PlaybackStateValue::Paused => "PAUSED",
            PlaybackStateValue::Starting => "STARTING",
            PlaybackStateValue::Ended => "ENDED",
            PlaybackStateValue::Advertisement => "ADVERTISEMENT",
            PlaybackStateValue::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaybackState {
    #[serde(rename = "currentTime", default)]
    pub current_time: String,
    #[serde(rename = "videoId", default)]
    pub video_id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub duration: String,
    #[serde(rename = "seekableStartTime", default)]
    pub seekable_start_time: String,
    #[serde(rename = "seekableEndTime", default)]
    pub seekable_end_time: String,
    #[serde(default)]
    pub volume: String,
    #[serde(default)]
    pub muted: String,
    #[serde(rename = "videoData", default, skip_deserializing)]
    pub video_data: VideoData,
    #[serde(default)]
    pub cpn: Option<String>,
    #[serde(rename = "loadedTime", default)]
    pub loaded_time: String,
}

impl PlaybackState {
    /// Get the parsed state value
    pub fn state_value(&self) -> i32 {
        self.state.parse::<i32>().unwrap_or(0)
    }

    /// Get the parsed current time
    pub fn current_time_value(&self) -> f64 {
        self.current_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed duration
    pub fn duration_value(&self) -> f64 {
        self.duration.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed volume
    pub fn volume_value(&self) -> i32 {
        self.volume.parse::<i32>().unwrap_or(0)
    }

    /// Get the parsed muted state
    pub fn is_muted(&self) -> bool {
        self.muted == "true"
    }

    /// Get the parsed seekable start time
    pub fn seekable_start_time_value(&self) -> f64 {
        self.seekable_start_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed seekable end time
    pub fn seekable_end_time_value(&self) -> f64 {
        self.seekable_end_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed loaded time
    pub fn loaded_time_value(&self) -> f64 {
        self.loaded_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Returns true if the video is currently playing
    pub fn is_playing(&self) -> bool {
        self.state_value() == PlaybackSession::PLAYING
    }

    /// Returns true if the video is currently paused
    pub fn is_paused(&self) -> bool {
        self.state_value() == PlaybackSession::PAUSED
    }

    /// Returns the state as a human-readable string
    pub fn state_name(&self) -> &'static str {
        match self.state_value() {
            PlaybackSession::STOPPED => "STOPPED",
            PlaybackSession::BUFFERING_BETWEEN => "BUFFERING_BETWEEN",
            PlaybackSession::PLAYING => "PLAYING",
            PlaybackSession::PAUSED => "PAUSED",
            PlaybackSession::STARTING => "STARTING",
            PlaybackSession::ADVERTISEMENT => "ADVERTISEMENT",
            _ => "UNKNOWN",
        }
    }
}

// Now playing event
#[derive(Debug, Clone, Deserialize)]
pub struct NowPlaying {
    #[serde(rename = "videoId", default)]
    pub video_id: String,
    #[serde(rename = "currentTime", default)]
    pub current_time: String,
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(rename = "videoData", default, skip_deserializing)]
    pub video_data: VideoData,
    #[serde(default)]
    pub cpn: Option<String>,
    #[serde(rename = "loadedTime", default)]
    pub loaded_time: String,
    #[serde(rename = "duration", default)]
    pub duration: String,
    #[serde(rename = "seekableStartTime", default)]
    pub seekable_start_time: String,
    #[serde(rename = "seekableEndTime", default)]
    pub seekable_end_time: String,
    #[serde(rename = "mdxExpandedReceiverVideoIdList", default)]
    pub mdx_expanded_receiver_video_id_list: Option<String>,
}

impl NowPlaying {
    /// Get the parsed state value
    pub fn state_value(&self) -> i32 {
        self.state.parse::<i32>().unwrap_or(0)
    }

    /// Get the parsed current time
    pub fn current_time_value(&self) -> f64 {
        self.current_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed duration
    pub fn duration_value(&self) -> f64 {
        self.duration.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed seekable start time
    pub fn seekable_start_time_value(&self) -> f64 {
        self.seekable_start_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed seekable end time
    pub fn seekable_end_time_value(&self) -> f64 {
        self.seekable_end_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the parsed loaded time
    pub fn loaded_time_value(&self) -> f64 {
        self.loaded_time.parse::<f64>().unwrap_or(0.0)
    }

    /// Get the video history from mdxExpandedReceiverVideoIdList
    pub fn video_history(&self) -> Option<Vec<String>> {
        self.mdx_expanded_receiver_video_id_list
            .as_ref()
            .map(|list| list.split(',').map(|s| s.trim().to_string()).collect())
    }
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
    #[serde(rename = "queueId", default)]
    pub queue_id: Option<String>,
}

// Ad state change event
#[derive(Debug, Clone, Deserialize)]
pub struct AdState {
    #[serde(rename = "contentVideoId")]
    pub content_video_id: String,
    #[serde(rename = "isSkipEnabled")]
    pub is_skip_enabled: bool,
}

impl AdState {
    /// Returns true if the ad can be skipped
    pub fn is_skippable(&self) -> bool {
        self.is_skip_enabled
    }

    /// Returns the ID of the video content that will play after the ad
    pub fn get_content_video_id(&self) -> &str {
        &self.content_video_id
    }
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

// Autoplay up next event
#[derive(Debug, Clone, Deserialize)]
pub struct AutoplayUpNext {
    #[serde(rename = "videoId")]
    pub video_id: String,
}

// Volume changed event
#[derive(Debug, Clone, Deserialize)]
pub struct VolumeChanged {
    pub muted: String,
    pub volume: String,
}

impl VolumeChanged {
    /// Returns true if audio is muted
    pub fn is_muted(&self) -> bool {
        self.muted == "true"
    }

    /// Returns the volume level as an integer (0-100)
    pub fn volume_level(&self) -> i32 {
        self.volume.parse::<i32>().unwrap_or(0)
    }
}
