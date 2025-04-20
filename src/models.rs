use crate::events::PlaybackStatus;
use crate::utils::youtube_parse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Screen {
    pub name: Option<String>,
    #[serde(rename = "screenId")]
    pub screen_id: String,
    #[serde(rename = "loungeToken")]
    pub lounge_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreenResponse {
    pub screen: Screen,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreensResponse {
    pub screens: Vec<Screen>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceInfo {
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub model: String,
    #[serde(rename = "deviceType", default)]
    pub device_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Device {
    pub app: String,
    pub name: String,
    pub id: String,
    #[serde(rename = "type")]
    pub device_type: String,
    #[serde(rename = "deviceInfo", default)]
    pub device_info_raw: String,
    #[serde(skip)]
    pub device_info: Option<DeviceInfo>,
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct PlaybackState {
    #[serde(rename = "currentTime", default)]
    pub current_time: String,
    #[serde(default = "default_state")]
    pub state: String,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub cpn: Option<String>,
    #[serde(rename = "loadedTime", default)]
    pub loaded_time: String,
}

// Helper function to provide default state value of "-1" (Stopped)
pub fn default_state() -> String {
    "-1".to_string()
}

impl PlaybackState {
    /// Get the current playback status as enum
    pub fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from(self.state.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NowPlaying {
    #[serde(rename = "videoId", default)]
    pub video_id: String,
    #[serde(rename = "currentTime", default)]
    pub current_time: String,
    #[serde(default = "default_state")]
    pub state: String,
    #[serde(rename = "videoData", default, skip_deserializing)]
    pub video_data: Option<VideoData>,
    #[serde(default)]
    pub cpn: Option<String>,
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    // Sometimes we have more fields
    #[serde(default)]
    pub duration: String,
    #[serde(rename = "loadedTime", default)]
    pub loaded_time: String,
    #[serde(rename = "seekableStartTime", default)]
    pub seekable_start_time: String,
    #[serde(rename = "seekableEndTime", default)]
    pub seekable_end_time: String,
}

impl NowPlaying {
    /// Get the current playback status as enum
    pub fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from(self.state.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdState {
    #[serde(rename = "AdState")]
    pub ad_state: String,
    #[serde(rename = "contentVideoId", default)]
    pub content_video_id: Option<String>,
    #[serde(rename = "currentTime")]
    pub current_time: String,
    #[serde(rename = "isSkipEnabled")]
    pub is_skip_enabled: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubtitlesTrackChanged {
    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioTrackChanged {
    #[serde(rename = "audioTrackId")]
    pub audio_track_id: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AutoplayModeChanged {
    #[serde(rename = "autoplayMode")]
    pub autoplay_mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HasPreviousNextChanged {
    #[serde(rename = "hasNext")]
    pub has_next: String,
    #[serde(rename = "hasPrevious")]
    pub has_previous: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoQualityChanged {
    #[serde(rename = "availableQualityLevels")]
    pub available_quality_levels: String,
    #[serde(rename = "qualityLevel")]
    pub quality_level: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolumeChanged {
    pub muted: String,
    pub volume: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistModified {
    #[serde(rename = "currentIndex", default)]
    pub current_index: Option<String>,
    #[serde(rename = "firstVideoId", default)]
    pub first_video_id: String,
    #[serde(rename = "listId", default)]
    pub list_id: String,
    #[serde(rename = "videoId", default)]
    pub video_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistModeChanged {
    #[serde(rename = "loopEnabled", default)]
    pub loop_enabled: String,
    #[serde(rename = "shuffleEnabled", default)]
    pub shuffle_enabled: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AutoplayUpNext {
    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoungeStatus {
    pub devices: String,
    #[serde(rename = "queueId", default)]
    pub queue_id: Option<String>,
}

// Helper methods for HasPreviousNextChanged
impl HasPreviousNextChanged {
    pub fn has_next(&self) -> bool {
        youtube_parse::parse_bool(&self.has_next)
    }

    pub fn has_previous(&self) -> bool {
        youtube_parse::parse_bool(&self.has_previous)
    }
}

// Helper methods for VideoQualityChanged
impl VideoQualityChanged {
    pub fn available_qualities(&self) -> Vec<String> {
        youtube_parse::parse_list(&self.available_quality_levels)
    }
}

// Helper methods for VolumeChanged
impl VolumeChanged {
    pub fn is_muted(&self) -> bool {
        youtube_parse::parse_bool(&self.muted)
    }

    pub fn volume_level(&self) -> i32 {
        youtube_parse::parse_int(&self.volume)
    }
}

// Helper methods for PlaylistModified
impl PlaylistModified {
    pub fn current_index_value(&self) -> Option<i32> {
        self.current_index
            .as_ref()
            .map(|idx| youtube_parse::parse_int(idx))
    }
}

// Helper methods for PlaylistModeChanged
impl PlaylistModeChanged {
    pub fn is_loop_enabled(&self) -> bool {
        youtube_parse::parse_bool(&self.loop_enabled)
    }

    pub fn is_shuffle_enabled(&self) -> bool {
        youtube_parse::parse_bool(&self.shuffle_enabled)
    }
}

// Helper methods for AdState
impl AdState {
    pub fn is_skippable(&self) -> bool {
        youtube_parse::parse_bool(&self.is_skip_enabled)
    }

    pub fn get_content_video_id(&self) -> &str {
        self.content_video_id.as_deref().unwrap_or("")
    }
}
