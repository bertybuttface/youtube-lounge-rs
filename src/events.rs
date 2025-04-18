use tracing::{debug, error, trace, warn};

use crate::error::LoungeError;
use crate::models;
use crate::state::InnerState;

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone)]
pub enum LoungeEvent {
    StateChange(models::PlaybackState),
    NowPlaying(models::NowPlaying),
    /// A synthetic event that combines NowPlaying and StateChange events
    /// for the same video (matched by CPN)
    PlaybackSession(PlaybackSession),
    LoungeStatus(Vec<models::Device>, Option<String>),
    ScreenDisconnected,
    SessionEstablished,
    AdStateChange(models::AdState),
    SubtitlesTrackChanged(models::SubtitlesTrackChanged),
    AudioTrackChanged(models::AudioTrackChanged),
    AutoplayModeChanged(models::AutoplayModeChanged),
    HasPreviousNextChanged(models::HasPreviousNextChanged),
    VideoQualityChanged(models::VideoQualityChanged),
    VolumeChanged(models::VolumeChanged),
    PlaylistModified(models::PlaylistModified),
    AutoplayUpNext(models::AutoplayUpNext),
    Unknown(String),
}

/// Represents a complete playback session with data combined from
/// NowPlaying and StateChange events.
#[derive(Debug, Clone)]
pub struct PlaybackSession {
    /// The unique YouTube video ID
    pub video_id: String,
    /// Current playback position in seconds
    pub current_time: f64,
    /// Total video duration in seconds
    pub duration: f64,
    /// Playback state (playing, paused, etc.)
    pub state: String,
    /// Detailed video metadata (may be None if not available yet)
    /// This requires separate API calls to populate
    pub video_data: Option<models::VideoData>,
    /// Client Playback Nonce - YouTube's internal ID for this playback session
    pub cpn: Option<String>,
    /// YouTube playlist ID if this video is part of a playlist
    pub list_id: Option<String>,
    /// Loaded time in seconds
    pub loaded_time: f64,
}

impl PlaybackSession {
    /// Get the current playback status as enum
    pub fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from(self.state.as_str())
    }

    /// Creates a new PlaybackSession from NowPlaying and StateChange events
    ///
    /// Uses the StateChange event for most playback state information and the
    /// NowPlaying event for additional context like playlist ID.
    pub fn new(
        now_playing: &models::NowPlaying,
        state: &models::PlaybackState,
    ) -> Result<Self, LoungeError> {
        let current_time = state
            .current_time
            .parse::<f64>()
            .map_err(LoungeError::NumericParseFailed)?;
        let duration = state
            .duration
            .parse::<f64>()
            .map_err(LoungeError::NumericParseFailed)?;
        let loaded_time = state
            .loaded_time
            .parse::<f64>()
            .map_err(LoungeError::NumericParseFailed)?;

        // Use the state from PlaybackState, or default to "-1" if empty
        let playback_state = if state.state.trim().is_empty() {
            models::default_state()
        } else {
            state.state.clone()
        };

        Ok(Self {
            video_id: now_playing.video_id.clone(),
            current_time,
            duration,
            state: playback_state,
            video_data: None,
            cpn: state.cpn.clone(),
            list_id: now_playing.list_id.clone(),
            loaded_time,
        })
    }
}

/// Represents the playback status codes from YouTube
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Stopped = -1,
    Buffering = 0,
    Playing = 1,
    Paused = 2,
    Starting = 3,
    Advertisement = 1081,
    Unknown = 9999,
}

impl From<&str> for PlaybackStatus {
    fn from(state: &str) -> Self {
        match state.parse::<i32>() {
            Ok(-1) => Self::Stopped,
            Ok(0) => Self::Buffering,
            Ok(1) => Self::Playing,
            Ok(2) => Self::Paused,
            Ok(3) => Self::Starting,
            Ok(1081) => Self::Advertisement,
            Ok(val) => {
                warn!("Unknown status value: {}", val);
                Self::Unknown
            }
            Err(_) => {
                warn!("Failed to parse status: {}", state);
                Self::Unknown
            }
        }
    }
}

impl std::fmt::Display for PlaybackStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stopped => "Stopped",
            Self::Buffering => "Buffering",
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Starting => "Starting",
            Self::Advertisement => "Advertisement",
            Self::Unknown => "Unknown",
        })
    }
}

pub(crate) async fn process_event_chunk(
    chunk: &str,
    sender: &broadcast::Sender<LoungeEvent>,
    latest_now_playing_arc: &Arc<RwLock<Option<models::NowPlaying>>>,
    _shared_state_arc: &Arc<RwLock<InnerState>>,
    aid_atomic: &Arc<AtomicU32>,
) {
    // Helper function for deserializing with error logging
    fn deserialize_with_logging<T>(
        event_type: &str,
        payload: &serde_json::Value,
    ) -> Result<T, serde_json::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        match serde_json::from_value::<T>(payload.clone()) {
            Ok(result) => Ok(result),
            Err(e) => {
                error!(event_type = %event_type, error = %e, "Failed to deserialize event");
                error!(payload = %payload, "Raw payload");
                Err(e)
            }
        }
    }

    // Helper for logging event details
    let log_event = |event_type: &str, payload: &serde_json::Value| {
        debug!(event_type = %event_type, payload = %payload, "Event received");
    };

    if chunk.trim().is_empty() {
        return;
    }

    let events = match serde_json::from_str::<Vec<Vec<serde_json::Value>>>(chunk) {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, raw_chunk = chunk, "Failed to parse event chunk JSON");
            return;
        }
    };

    for event in events {
        if event.len() < 2 {
            continue;
        }
        if let Some(event_id) = event.first().and_then(|id| id.as_i64()) {
            aid_atomic.store(event_id as u32, Ordering::SeqCst);
        }

        if let Some(event_array) = event.get(1).and_then(|v| v.as_array()) {
            // Check for the specific JSON noop structure [[N, ["noop"]]]
            if event_array.len() == 1 {
                // Should only contain ["noop"]
                if let Some(event_type) = event_array.first().and_then(|t| t.as_str()) {
                    if event_type == "noop" {
                        trace!("Received JSON noop event, connection alive.");
                        continue; // Skip further processing for this specific event
                    } else {
                        debug!(event_type = %event_type, "Received single-element event array");
                    }
                }
            }
            if event_array.len() < 2 {
                continue;
            }
            if let Some(event_type) = event_array.first().and_then(|t| t.as_str()) {
                let payload = &event_array[1];
                log_event(event_type, payload);

                match event_type {
                    "onStateChange" => {
                        if let Ok(state) =
                            deserialize_with_logging::<models::PlaybackState>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::StateChange(state.clone()));
                            let latest_np = {
                                let guard = latest_now_playing_arc.read().await;
                                guard.clone()
                            };
                            if let Some(np) = latest_np.as_ref() {
                                if let (Some(state_cpn), Some(np_cpn)) = (&state.cpn, &np.cpn) {
                                    if state_cpn == np_cpn {
                                        if let Ok(session) = PlaybackSession::new(np, &state) {
                                            let _ =
                                                sender.send(LoungeEvent::PlaybackSession(session));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "nowPlaying" => {
                        if let Ok(now_playing) =
                            deserialize_with_logging::<models::NowPlaying>(event_type, payload)
                        {
                            debug!(
                                "NowPlaying: id={} state={} time={}/{} list={} cpn={}",
                                now_playing.video_id,
                                now_playing.state,
                                now_playing.current_time,
                                now_playing.duration,
                                now_playing.list_id.as_deref().unwrap_or("-"),
                                now_playing.cpn.as_deref().unwrap_or("-")
                            );

                            // Always send the raw event
                            let _ = sender.send(LoungeEvent::NowPlaying(now_playing.clone()));
                            if now_playing.cpn.is_some() {
                                let mut guard = latest_now_playing_arc.write().await;
                                *guard = Some(now_playing.clone());
                            }
                            // Create and send a PlaybackSession if possible
                            match now_playing.state.as_str() {
                                // Handle stop events (-1)
                                "-1" if now_playing.video_id.is_empty() => {
                                    let prev_np_opt = {
                                        let guard = latest_now_playing_arc.read().await;
                                        guard.clone()
                                    };
                                    if let Some(prev) = prev_np_opt.as_ref() {
                                        // Use prev_np_opt
                                        let state = models::PlaybackState {
                                            current_time: "0".to_string(),
                                            state: "-1".to_string(),
                                            duration: prev.duration.clone(),
                                            cpn: prev.cpn.clone(),
                                            loaded_time: "0".to_string(),
                                        };

                                        if let Ok(session) = PlaybackSession::new(prev, &state) {
                                            let _ =
                                                sender.send(LoungeEvent::PlaybackSession(session));
                                        }
                                    }
                                }

                                // Handle normal events with sufficient data
                                _ if !now_playing.video_id.is_empty()
                                    && !now_playing.duration.is_empty()
                                    && !now_playing.current_time.is_empty() =>
                                {
                                    let state_from_np = models::PlaybackState {
                                        current_time: now_playing.current_time.clone(),
                                        state: now_playing.state.clone(),
                                        duration: now_playing.duration.clone(),
                                        cpn: now_playing.cpn.clone(),
                                        loaded_time: now_playing.loaded_time.clone(),
                                    };
                                    if let Ok(session) =
                                        PlaybackSession::new(&now_playing, &state_from_np)
                                    {
                                        let _ = sender.send(LoungeEvent::PlaybackSession(session));
                                    }
                                }

                                _ => debug!("Insufficient data to create PlaybackSession"),
                            }
                        }
                    }
                    "loungeStatus" => {
                        if let Ok(status) =
                            deserialize_with_logging::<models::LoungeStatus>(event_type, payload)
                        {
                            match serde_json::from_str::<Vec<models::Device>>(&status.devices) {
                                Ok(devices) => {
                                    // Process devices (parse device_info if available)
                                    let devices_with_info: Vec<models::Device> = devices
                                        .into_iter()
                                        .map(|mut device| {
                                            if !device.device_info_raw.trim().is_empty() {
                                                match serde_json::from_str::<models::DeviceInfo>(
                                                    &device.device_info_raw,
                                                ) {
                                                    Ok(info) => {
                                                        device.device_info = Some(info);
                                                    }
                                                    Err(e) => {
                                                        error!(
                                                            error = %e,
                                                            "Failed to parse device_info"
                                                        );
                                                        error!(
                                                            raw_info = %device.device_info_raw,
                                                            "Raw device_info"
                                                        );
                                                    }
                                                }
                                            }
                                            device
                                        })
                                        .collect();

                                    let _ = sender.send(LoungeEvent::LoungeStatus(
                                        devices_with_info,
                                        status.queue_id,
                                    ));
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to parse devices from loungeStatus");
                                    error!(devices = %status.devices, "Raw devices string");
                                }
                            }
                        }
                    }
                    "loungeScreenDisconnected" => {
                        let _ = sender.send(LoungeEvent::ScreenDisconnected);
                    }
                    "onAdStateChange" => {
                        if let Ok(state) =
                            deserialize_with_logging::<models::AdState>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::AdStateChange(state));
                        }
                    }
                    "onSubtitlesTrackChanged" => {
                        if let Ok(state) = deserialize_with_logging::<models::SubtitlesTrackChanged>(
                            event_type, payload,
                        ) {
                            let _ = sender.send(LoungeEvent::SubtitlesTrackChanged(state));
                        }
                    }
                    "onAudioTrackChanged" => {
                        if let Ok(state) = deserialize_with_logging::<models::AudioTrackChanged>(
                            event_type, payload,
                        ) {
                            let _ = sender.send(LoungeEvent::AudioTrackChanged(state));
                        }
                    }
                    "onAutoplayModeChanged" => {
                        if let Ok(state) = deserialize_with_logging::<models::AutoplayModeChanged>(
                            event_type, payload,
                        ) {
                            let _ = sender.send(LoungeEvent::AutoplayModeChanged(state));
                        }
                    }
                    "onHasPreviousNextChanged" => {
                        if let Ok(state) = deserialize_with_logging::<models::HasPreviousNextChanged>(
                            event_type, payload,
                        ) {
                            let _ = sender.send(LoungeEvent::HasPreviousNextChanged(state));
                        }
                    }
                    "onVideoQualityChanged" => {
                        if let Ok(state) = deserialize_with_logging::<models::VideoQualityChanged>(
                            event_type, payload,
                        ) {
                            let _ = sender.send(LoungeEvent::VideoQualityChanged(state));
                        }
                    }
                    "onVolumeChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<models::VolumeChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::VolumeChanged(state));
                        }
                    }
                    "playlistModified" => {
                        if let Ok(state) = deserialize_with_logging::<models::PlaylistModified>(
                            event_type, payload,
                        ) {
                            let _ = sender.send(LoungeEvent::PlaylistModified(state));
                        }
                    }
                    "autoplayUpNext" => {
                        if let Ok(state) =
                            deserialize_with_logging::<models::AutoplayUpNext>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::AutoplayUpNext(state));
                        }
                    }
                    _ => {
                        let event_with_payload = format!("{} - payload: {}", event_type, payload);
                        warn!(
                            "Unknown event type '{}' with payload: {}",
                            event_type, payload
                        );
                        let _ = sender.send(LoungeEvent::Unknown(event_with_payload));
                    }
                }
            }
        }
    }
}
