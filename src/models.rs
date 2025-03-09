use serde::Deserialize;
use std::time::Instant;

// Import our utility traits
use crate::utils::parsing::{HasDuration, HasVolume, YoutubeValueParser};
use crate::utils::state::{HasPlaybackState, PlaybackStatus};

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
    pub device_id: Option<String>,
}

impl PlaybackSession {
    // Create a new session from a NowPlaying event
    pub fn from_now_playing(event: &NowPlaying) -> Option<Self> {
        let cpn = event.cpn.clone()?;

        Some(Self {
            cpn,
            video_id: Some(event.video_id.clone()),
            list_id: event.list_id.clone(),
            current_time: event.current_time(),
            duration: event.duration(),
            state: PlaybackStatus::from(event).to_i32(),
            loaded_time: event.loaded_time(),
            seekable_start_time: event.seekable_start_time(),
            seekable_end_time: event.seekable_end_time(),
            last_updated: Instant::now(),
            video_history: event.video_history(),
            device_id: None, // Will be populated later when we have device mapping
        })
    }

    // Update from a StateChange event
    pub fn update_from_state_change(&mut self, event: &PlaybackState) -> bool {
        let mut updated = false;

        // Update current time
        let new_current_time = event.current_time();
        if self.current_time != new_current_time {
            self.current_time = new_current_time;
            updated = true;
        }

        // Update duration
        let new_duration = event.duration();
        if self.duration != new_duration {
            self.duration = new_duration;
            updated = true;
        }

        // Update state
        let new_state = PlaybackStatus::from(event).to_i32();
        if self.state != new_state {
            self.state = new_state;
            updated = true;
        }

        // Update seekable times
        let new_seekable_start = event.seekable_start_time();
        if self.seekable_start_time != new_seekable_start {
            self.seekable_start_time = new_seekable_start;
            updated = true;
        }

        let new_seekable_end = event.seekable_end_time();
        if self.seekable_end_time != new_seekable_end {
            self.seekable_end_time = new_seekable_end;
            updated = true;
        }

        // Update loaded time
        let new_loaded_time = event.loaded_time();
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
            current_time: event.current_time(),
            duration: event.duration(),
            state: PlaybackStatus::from(event).to_i32(),
            loaded_time: event.loaded_time(),
            seekable_start_time: event.seekable_start_time(),
            seekable_end_time: event.seekable_end_time(),
            last_updated: Instant::now(),
            video_history: None,
            device_id: None, // Will be populated later when we have device mapping
        })
    }

    // Get the playback status
    pub fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from_i32(self.state)
    }

    // Get status name
    pub fn state_name(&self) -> &'static str {
        self.status().as_str()
    }

    // Delegate to PlaybackStatus for state checking
    pub fn is_playing(&self) -> bool {
        self.status() == PlaybackStatus::Playing
    }

    pub fn is_paused(&self) -> bool {
        self.status() == PlaybackStatus::Paused
    }

    pub fn is_ad(&self) -> bool {
        self.status() == PlaybackStatus::Advertisement
    }

    pub fn is_buffering(&self) -> bool {
        self.status() == PlaybackStatus::BufferingBetween
    }

    pub fn is_stopped(&self) -> bool {
        self.status() == PlaybackStatus::Stopped
    }

    pub fn progress_percentage(&self) -> f64 {
        if self.duration > 0.0 {
            (self.current_time / self.duration) * 100.0
        } else {
            0.0
        }
    }
}

// Implement HasPlaybackState for PlaybackSession
impl HasPlaybackState for PlaybackSession {
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from_i32(self.state)
    }
}

// Implement HasDuration for PlaybackSession
impl HasDuration for PlaybackSession {
    fn current_time(&self) -> f64 {
        self.current_time
    }

    fn duration(&self) -> f64 {
        self.duration
    }

    fn loaded_time(&self) -> f64 {
        self.loaded_time
    }

    fn seekable_start_time(&self) -> f64 {
        self.seekable_start_time
    }

    fn seekable_end_time(&self) -> f64 {
        self.seekable_end_time
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
///
/// NOTE: This is kept for backward compatibility but
/// should be replaced with PlaybackStatus in new code.
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

// Convert between PlaybackStateValue and PlaybackStatus
impl From<PlaybackStateValue> for PlaybackStatus {
    fn from(value: PlaybackStateValue) -> Self {
        match value {
            PlaybackStateValue::Stopped => PlaybackStatus::Stopped,
            PlaybackStateValue::BufferingBetween => PlaybackStatus::BufferingBetween,
            PlaybackStateValue::Playing => PlaybackStatus::Playing,
            PlaybackStateValue::Paused => PlaybackStatus::Paused,
            PlaybackStateValue::Starting => PlaybackStatus::Starting,
            PlaybackStateValue::Ended => PlaybackStatus::Ended,
            PlaybackStateValue::Advertisement => PlaybackStatus::Advertisement,
            PlaybackStateValue::Unknown => PlaybackStatus::Unknown,
        }
    }
}

impl From<PlaybackStatus> for PlaybackStateValue {
    fn from(status: PlaybackStatus) -> Self {
        match status {
            PlaybackStatus::Stopped => PlaybackStateValue::Stopped,
            PlaybackStatus::BufferingBetween => PlaybackStateValue::BufferingBetween,
            PlaybackStatus::Playing => PlaybackStateValue::Playing,
            PlaybackStatus::Paused => PlaybackStateValue::Paused,
            PlaybackStatus::Starting => PlaybackStateValue::Starting,
            PlaybackStatus::Ended => PlaybackStateValue::Ended,
            PlaybackStatus::Advertisement => PlaybackStateValue::Advertisement,
            PlaybackStatus::Unknown => PlaybackStateValue::Unknown,
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
    // Legacy methods for backward compatibility

    /// Get the parsed state value
    pub fn state_value(&self) -> i32 {
        <str as YoutubeValueParser>::parse_int(&self.state)
    }

    /// Get the parsed current time
    pub fn current_time_value(&self) -> f64 {
        self.current_time()
    }

    /// Get the parsed duration
    pub fn duration_value(&self) -> f64 {
        self.duration()
    }

    /// Get the parsed volume
    pub fn volume_value(&self) -> i32 {
        self.volume()
    }

    /// Get the parsed loaded time
    pub fn loaded_time_value(&self) -> f64 {
        self.loaded_time()
    }

    /// Get the parsed seekable start time
    pub fn seekable_start_time_value(&self) -> f64 {
        self.seekable_start_time()
    }

    /// Get the parsed seekable end time
    pub fn seekable_end_time_value(&self) -> f64 {
        self.seekable_end_time()
    }

    /// Returns the state as a human-readable string
    pub fn state_name(&self) -> &'static str {
        PlaybackStatus::from(self).as_str()
    }
}

// Implement our traits for PlaybackState
impl HasPlaybackState for PlaybackState {
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from_i32(self.state_value())
    }
}

impl From<&PlaybackState> for PlaybackStatus {
    fn from(state: &PlaybackState) -> Self {
        PlaybackStatus::from_i32(<str as YoutubeValueParser>::parse_int(&state.state))
    }
}

impl HasDuration for PlaybackState {
    fn current_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.current_time)
    }

    fn duration(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.duration)
    }

    fn loaded_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.loaded_time)
    }

    fn seekable_start_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.seekable_start_time)
    }

    fn seekable_end_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.seekable_end_time)
    }
}

impl HasVolume for PlaybackState {
    fn volume(&self) -> i32 {
        <str as YoutubeValueParser>::parse_int(&self.volume)
    }

    fn is_muted(&self) -> bool {
        <str as YoutubeValueParser>::parse_bool(&self.muted)
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
    // Legacy methods for backward compatibility

    /// Get the parsed state value
    pub fn state_value(&self) -> i32 {
        <str as YoutubeValueParser>::parse_int(&self.state)
    }

    /// Get the parsed current time
    pub fn current_time_value(&self) -> f64 {
        self.current_time()
    }

    /// Get the parsed duration
    pub fn duration_value(&self) -> f64 {
        self.duration()
    }

    /// Get the parsed seekable start time
    pub fn seekable_start_time_value(&self) -> f64 {
        self.seekable_start_time()
    }

    /// Get the parsed seekable end time
    pub fn seekable_end_time_value(&self) -> f64 {
        self.seekable_end_time()
    }

    /// Get the parsed loaded time
    pub fn loaded_time_value(&self) -> f64 {
        self.loaded_time()
    }

    /// Get the video history from mdxExpandedReceiverVideoIdList
    pub fn video_history(&self) -> Option<Vec<String>> {
        self.mdx_expanded_receiver_video_id_list
            .as_ref()
            .map(|list| <str as YoutubeValueParser>::parse_list(list))
    }
}

// Implement our traits for NowPlaying
impl HasPlaybackState for NowPlaying {
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from_i32(self.state_value())
    }
}

impl From<&NowPlaying> for PlaybackStatus {
    fn from(np: &NowPlaying) -> Self {
        PlaybackStatus::from_i32(<str as YoutubeValueParser>::parse_int(&np.state))
    }
}

impl HasDuration for NowPlaying {
    fn current_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.current_time)
    }

    fn duration(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.duration)
    }

    fn loaded_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.loaded_time)
    }

    fn seekable_start_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.seekable_start_time)
    }

    fn seekable_end_time(&self) -> f64 {
        <str as YoutubeValueParser>::parse_float(&self.seekable_end_time)
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
    pub id: String, // This is the device ID
    #[serde(rename = "type")]
    pub device_type: String,
    #[serde(rename = "deviceInfo", default)]
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

impl HasPreviousNextChanged {
    /// Check if there is a next video
    pub fn has_next(&self) -> bool {
        <str as YoutubeValueParser>::parse_bool(&self.has_next)
    }

    /// Check if there is a previous video
    pub fn has_previous(&self) -> bool {
        <str as YoutubeValueParser>::parse_bool(&self.has_previous)
    }
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

impl VideoQualityChanged {
    /// Get available quality levels as a vector
    pub fn available_qualities(&self) -> Vec<String> {
        <str as YoutubeValueParser>::parse_list(&self.available_quality_levels)
    }
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
    #[serde(rename = "currentIndex", default)]
    pub current_index: Option<String>,
    #[serde(rename = "firstVideoId")]
    pub first_video_id: String,
    #[serde(rename = "listId")]
    pub list_id: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
}

impl PlaylistModified {
    /// Get the current index as an integer, returns None if not present
    pub fn current_index_value(&self) -> Option<i32> {
        self.current_index
            .as_ref()
            .map(|idx| <str as YoutubeValueParser>::parse_int(idx))
    }
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
        <str as YoutubeValueParser>::parse_bool(&self.muted)
    }

    /// Returns the volume level as an integer (0-100)
    pub fn volume_level(&self) -> i32 {
        <str as YoutubeValueParser>::parse_int(&self.volume)
    }
}

// Implement HasVolume for VolumeChanged
impl HasVolume for VolumeChanged {
    fn volume(&self) -> i32 {
        self.volume_level()
    }

    fn is_muted(&self) -> bool {
        self.is_muted()
    }
}
