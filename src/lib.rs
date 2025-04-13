use bytes::BytesMut;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{sleep, timeout, Duration};
use tokio_util::codec::Decoder;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

const BUFFER_CAPACITY: usize = 16 * 1024; // 16KB initial buffer capacity
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(32); // Wait 32s for next chunk

// Type alias for the optional callback function pointer for clarity
type TokenCallback = Option<Box<dyn Fn(&str, &str) + Send + Sync + 'static>>;

struct InnerState {
    lounge_token: String,
    token_refresh_callback: TokenCallback,
}

// Basic error handling with thiserror
#[derive(Error, Debug)]
pub enum LoungeError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("JSON parsing failed: {0}")]
    ParseFailed(#[from] serde_json::Error),

    #[error("URL encoding failed: {0}")]
    UrlEncodingFailed(#[from] serde_urlencoded::ser::Error),

    #[error("Numeric parsing failed: {0}")]
    NumericParseFailed(#[from] std::num::ParseFloatError),

    #[error("Session expired")]
    SessionExpired,

    #[error("Token expired")]
    TokenExpired,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

// Custom codec for YouTube Lounge API protocol
// Handles the format: <text length>\n<message content>\n
struct LoungeCodec {
    // Current parsing state
    state: LoungeCodecState,
}

enum LoungeCodecState {
    // Waiting for a line containing the size
    ReadingSize,
    // Found size, now reading content
    ReadingContent { expected_size: usize },
}

impl LoungeCodec {
    fn new() -> Self {
        Self {
            state: LoungeCodecState::ReadingSize,
        }
    }
}

impl Decoder for LoungeCodec {
    type Item = String;
    type Error = std::io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match &mut self.state {
                LoungeCodecState::ReadingSize => {
                    // Look for a newline to delimit the size
                    if let Some(newline_pos) = buf.iter().position(|&b| b == b'\n') {
                        // Extract the size line (including the newline)
                        let line = buf.split_to(newline_pos + 1);

                        // Convert to UTF-8 and trim
                        let size_str =
                            std::str::from_utf8(&line[..line.len() - 1]).map_err(|_| {
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "Invalid UTF-8 in size header",
                                )
                            })?;
                        let size_str = size_str.trim();

                        // Ensure it’s numeric
                        if !size_str.chars().all(|c| c.is_ascii_digit()) {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Expected numeric size, got: {}", size_str),
                            ));
                        }

                        // Parse to usize
                        let expected_size = size_str.parse::<usize>().map_err(|_| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Invalid size: {}", size_str),
                            )
                        })?;

                        // Move to next state
                        self.state = LoungeCodecState::ReadingContent { expected_size };

                        // Continue loop to handle content immediately
                        continue;
                    }

                    // Not enough data for a full size line
                    return Ok(None);
                }

                LoungeCodecState::ReadingContent { expected_size } => {
                    // Wait for enough data
                    if buf.len() >= *expected_size {
                        let content = buf.split_to(*expected_size);

                        let message = String::from_utf8(content.to_vec()).map_err(|_| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid UTF-8 in message content",
                            )
                        })?;

                        // Reset state
                        self.state = LoungeCodecState::ReadingSize;

                        return Ok(Some(message));
                    }

                    // Wait for more data
                    return Ok(None);
                }
            }
        }
    }
}

// Model Structs

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
fn default_state() -> String {
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

// Playback Command Enum
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Next,
    Previous,
    SkipAd,
    SetPlaylist {
        video_id: String,
        list_id: Option<String>,
        current_index: Option<i32>,
        current_time: Option<f64>,
        audio_only: Option<bool>,
        params: Option<String>,
        player_params: Option<String>,
    },
    AddVideo {
        video_id: String,
        video_sources: Option<String>,
    },
    SeekTo {
        new_time: f64,
    },
    SetAutoplayMode {
        autoplay_mode: String,
    },
    SetVolume {
        volume: i32,
    },
    Mute,
    Unmute,
}

impl PlaybackCommand {
    pub fn set_playlist(video_id: String) -> Self {
        PlaybackCommand::SetPlaylist {
            video_id,
            list_id: None,
            current_index: Some(-1),
            current_time: Some(0.0),
            audio_only: Some(false),
            params: None,
            player_params: None,
        }
    }

    pub fn set_playlist_by_id(list_id: String) -> Self {
        PlaybackCommand::SetPlaylist {
            video_id: "".to_string(),
            list_id: Some(list_id),
            current_index: Some(0),
            current_time: Some(0.0),
            audio_only: Some(false),
            params: None,
            player_params: None,
        }
    }

    pub fn set_playlist_with_index(list_id: String, index: i32) -> Self {
        PlaybackCommand::SetPlaylist {
            video_id: "".to_string(),
            list_id: Some(list_id),
            current_index: Some(index),
            current_time: Some(0.0),
            audio_only: Some(false),
            params: None,
            player_params: None,
        }
    }

    pub fn add_video(video_id: String) -> Self {
        PlaybackCommand::AddVideo {
            video_id,
            video_sources: None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Pause => "pause",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::SkipAd => "skipAd",
            Self::SetPlaylist { .. } => "setPlaylist",
            Self::AddVideo { .. } => "addVideo",
            Self::SeekTo { .. } => "seekTo",
            Self::SetAutoplayMode { .. } => "setAutoplayMode",
            Self::SetVolume { .. } => "setVolume",
            Self::Mute => "mute",
            Self::Unmute => "unMute",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdState {
    #[serde(rename = "contentVideoId")]
    pub content_video_id: String,
    #[serde(rename = "isSkipEnabled")]
    pub is_skip_enabled: bool,
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

// Events
#[derive(Debug, Clone)]
pub enum LoungeEvent {
    StateChange(PlaybackState),
    NowPlaying(NowPlaying),
    /// A synthetic event that combines NowPlaying and StateChange events
    /// for the same video (matched by CPN)
    PlaybackSession(PlaybackSession),
    LoungeStatus(Vec<Device>, Option<String>),
    ScreenDisconnected,
    SessionEstablished,
    AdStateChange(AdState),
    SubtitlesTrackChanged(SubtitlesTrackChanged),
    AudioTrackChanged(AudioTrackChanged),
    AutoplayModeChanged(AutoplayModeChanged),
    HasPreviousNextChanged(HasPreviousNextChanged),
    VideoQualityChanged(VideoQualityChanged),
    VolumeChanged(VolumeChanged),
    PlaylistModified(PlaylistModified),
    AutoplayUpNext(AutoplayUpNext),
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
    pub video_data: Option<VideoData>,
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
    pub fn new(now_playing: &NowPlaying, state: &PlaybackState) -> Result<Self, LoungeError> {
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
            default_state()
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

#[derive(Clone)]
struct SessionState {
    sid: Option<String>,
    gsessionid: Option<String>,
    rid: Arc<AtomicU32>,
    command_offset: Arc<AtomicU32>,
}

impl SessionState {
    fn new() -> Self {
        Self {
            sid: None,
            gsessionid: None,
            rid: Arc::new(AtomicU32::new(1)),
            command_offset: Arc::new(AtomicU32::new(0)),
        }
    }

    fn increment_rid(&self) -> u32 {
        self.rid.fetch_add(1, Ordering::SeqCst)
    }
    fn increment_offset(&self) -> u32 {
        self.command_offset.fetch_add(1, Ordering::SeqCst)
    }
}

/// Main client enables controlling YouTube playback on TV devices through
/// the YouTube Lounge API protocol. It handles pairing, authentication,
/// session management, and sending commands to control playback.
///
/// # Logging
///
/// This library uses the `tracing` crate for logging. To enable logs, you'll need to
/// initialize a tracing subscriber in your application.
///
/// Example using `tracing_subscriber`:
/// ```no_run
/// use tracing::Level;
/// use tracing_subscriber::FmtSubscriber;
///
/// // Create a subscriber with the desired log level
/// let subscriber = FmtSubscriber::builder()
///     .with_max_level(Level::DEBUG) // Set to DEBUG, INFO, WARN, or ERROR
///     .finish();
///
/// // Initialize the global subscriber
/// tracing::subscriber::set_global_default(subscriber)
///     .expect("Failed to set tracing subscriber");
/// ```
///
/// The log levels control what information is displayed:
/// - `DEBUG`: Shows detailed information about network requests, message parsing, etc.
/// - `INFO`: Shows high-level operations and successful connections
/// - `WARN`: Shows warnings and non-critical errors
/// - `ERROR`: Shows critical failures and error conditions
pub struct LoungeClient {
    client: Arc<Client>,
    device_id: String,
    screen_id: String,
    device_name: String,
    session_state: SessionState,
    event_sender: broadcast::Sender<LoungeEvent>,
    connected: bool,
    // Track latest NowPlaying with CPN for PlaybackSession generation
    latest_now_playing: Arc<RwLock<Option<NowPlaying>>>,
    shared_state: Arc<RwLock<InnerState>>,
    aid_atomic: Arc<AtomicU32>,
}

impl LoungeClient {
    /// Create a new LoungeClient. If a device_id is provided, it will be used;
    /// otherwise, a new UUID is generated. Optionally accepts a custom reqwest client
    /// for connection reuse and shared configuration.
    pub fn new(
        screen_id: &str,
        lounge_token: &str,
        device_name: &str,
        device_id: Option<&str>,
        custom_client: Option<Arc<Client>>,
    ) -> Self {
        let client = custom_client.unwrap_or_else(|| {
            Arc::new(
                Client::builder()
                    .pool_idle_timeout(Some(Duration::from_secs(600)))
                    .pool_max_idle_per_host(256)
                    .build()
                    .unwrap(),
            )
        });
        let device_id = device_id.map_or_else(|| Uuid::new_v4().to_string(), ToString::to_string);
        let (event_tx, _) = broadcast::channel(100);

        // Initialize the inner state for the Mutex
        let initial_state = InnerState {
            lounge_token: lounge_token.to_string(),
            token_refresh_callback: None, // Will be set later via method
        };

        Self {
            client,
            device_id,
            screen_id: screen_id.to_string(),
            device_name: device_name.to_string(),
            session_state: SessionState::new(),
            event_sender: event_tx,
            connected: false,
            latest_now_playing: Arc::new(RwLock::new(None)),
            shared_state: Arc::new(RwLock::new(initial_state)),
            aid_atomic: Arc::new(AtomicU32::new(0)),
        }
    }

    pub async fn set_token_refresh_callback<F>(&self, callback: F)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        let mut state_guard = self.shared_state.write().await;
        state_guard.token_refresh_callback = Some(Box::new(callback));
        debug!("Token refresh callback set.");
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn screen_id(&self) -> &str {
        &self.screen_id
    }

    pub fn event_receiver(&self) -> broadcast::Receiver<LoungeEvent> {
        self.event_sender.subscribe()
    }

    /// Pair with a screen using a pairing code displayed on the TV
    pub async fn pair_with_screen(pairing_code: &str) -> Result<Screen, LoungeError> {
        info!("Pairing with screen using code: {}", pairing_code);
        let client = Client::new();
        let params = [("pairing_code", pairing_code)];

        let response = client
            .post("https://www.youtube.com/api/lounge/pairing/get_screen")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_msg = format!("Failed to pair with screen: {}", response.status());
            error!("{}", error_msg);
            return Err(LoungeError::InvalidResponse(error_msg));
        }

        let screen_response = response.json::<ScreenResponse>().await?;
        info!(
            "Successfully paired with screen: {}",
            screen_response
                .screen
                .name
                .as_deref()
                .unwrap_or("<unnamed>")
        );
        Ok(screen_response.screen)
    }

    /// Refresh the lounge token for a screen
    pub async fn refresh_lounge_token(screen_id: &str) -> Result<Screen, LoungeError> {
        info!("Refreshing lounge token for screen_id: {}", screen_id);
        let client = Client::new();
        let params = [("screen_ids", screen_id)];

        let response = client
            .post("https://www.youtube.com/api/lounge/pairing/get_lounge_token_batch")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let error_msg = format!("Failed to refresh token: {}: {}", status, body_text);
            error!("{}", error_msg);
            return Err(LoungeError::InvalidResponse(error_msg));
        }

        let screens_response = response.json::<ScreensResponse>().await?;

        let screen = screens_response
            .screens
            .into_iter()
            .next()
            .ok_or_else(|| LoungeError::InvalidResponse("No screens returned".to_string()))?;

        debug!(
            "Token refreshed successfully for screen: {}",
            screen.name.as_deref().unwrap_or("<unnamed>")
        );

        Ok(screen)
    }

    /// Check if a screen is available using the current lounge token
    pub async fn check_screen_availability(&self) -> Result<bool, LoungeError> {
        debug!(
            "Checking screen availability for screen_id: {}",
            self.screen_id
        );

        let token = {
            let state_guard = self.shared_state.read().await;
            state_guard.lounge_token.clone()
        };
        let params = [("lounge_token", &token)];
        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/pairing/get_screen_availability")
            .form(&params)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            warn!("Token expired for screen_id: {}", self.screen_id);
            return Err(LoungeError::TokenExpired);
        }

        let available = response.status().is_success();
        debug!("Screen availability: {}", available);

        Ok(available)
    }

    /// Check screen availability with automatic token refresh if needed
    pub async fn check_screen_availability_with_refresh(&self) -> Result<bool, LoungeError> {
        match self.check_screen_availability().await {
            Ok(available) => Ok(available),
            Err(LoungeError::TokenExpired) => {
                info!("Refreshing expired token (check_screen_availability_with_refresh)");
                let screen = Self::refresh_lounge_token(&self.screen_id).await?;
                {
                    let mut state = self.shared_state.write().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                }
                self.check_screen_availability().await
            }
            Err(e) => Err(e),
        }
    }

    /// Connect to the screen and establish a session
    pub async fn connect(&mut self) -> Result<(), LoungeError> {
        info!("Connecting to screen: {}", self.screen_id);

        // Reset session state
        debug!("Resetting SessionState due to connect.");
        self.session_state = SessionState::new();

        let params = [
            ("RID", "1"),
            ("VER", "8"),
            ("CVER", "1"),
            ("auth_failure_option", "send_error"),
            // ("TYPE", " Anmeldung"), // Use the special " Anmeldung" TYPE for initial bind
            // ("app", "youtube-desktop"), // Maybe not needed here? Check captures.
            // ("device", "REMOTE_CONTROL"), // Specify device type
            // ("id", self.device_id.as_str()), // Specify device id
            // ("name", self.device_name.as_str()), // Specify device name
        ];

        let form_data = self.build_connect_form_data().await?;
        debug!("Sending initial connection request");

        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await?;

        // Check for specific connect errors before reading body
        match response.status().as_u16() {
            401 => {
                error!("Connection failed: 401 Unauthorized. Token is likely invalid or expired.");
                return Err(LoungeError::TokenExpired); // Specific error
            }
            404 => {
                error!("Connection failed: 404 Not Found. Screen ID might be invalid or unpaired.");
                return Err(LoungeError::InvalidResponse(
                    "Screen not found (404)".to_string(),
                ));
            }
            status if !response.status().is_success() => {
                let body_text = response.text().await.unwrap_or_default();
                let error_msg = format!("Failed to connect: {}: {}", status, body_text);
                error!("{}", error_msg);
                return Err(LoungeError::InvalidResponse(error_msg));
            }
            _ => {} // Success, proceed
        }

        let body = response.bytes().await?;

        // Extract session IDs
        debug!("Extracting session IDs from response");
        let (sid, gsessionid) = extract_session_ids(&body)?;

        // Update session state (local to this LoungeClient instance)
        self.session_state.sid = sid.clone();
        self.session_state.gsessionid = gsessionid.clone();
        self.connected = true;

        debug!(
            "Session established with SID: {}",
            sid.as_deref().unwrap_or("<none>")
        );

        let _ = self.event_sender.send(LoungeEvent::SessionEstablished);
        self.subscribe_to_events_grouped().await?;
        Ok(())
    }

    /// Connect to the screen with automatic token refresh if needed
    pub async fn connect_with_refresh(&mut self) -> Result<(), LoungeError> {
        match self.connect().await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                info!("Refreshing expired token (connect_with_refresh)");
                let screen = Self::refresh_lounge_token(&self.screen_id).await?;
                {
                    let mut state = self.shared_state.write().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                    debug!("Retrying connect");
                }
                self.connect().await
            }
            Err(e) => Err(e),
        }
    }

    async fn subscribe_to_events_grouped(&self) -> Result<(), LoungeError> {
        #[derive(Clone)]
        struct TaskArcs {
            latest_now_playing: Arc<RwLock<Option<NowPlaying>>>,
            shared_state: Arc<RwLock<InnerState>>,
            aid_atomic: Arc<AtomicU32>,
        }
        let arcs = TaskArcs {
            latest_now_playing: self.latest_now_playing.clone(),
            shared_state: self.shared_state.clone(),
            aid_atomic: self.aid_atomic.clone(),
        };
        let client = self.client.clone();
        let device_name = self.device_name.clone();
        let screen_id = self.screen_id.clone();
        let event_sender = self.event_sender.clone();
        let session_state = self.session_state.clone();
        tokio::spawn(async move {
            debug!("Starting event subscriber task");
            loop {
                // Check task's local session validity
                let (sid, gsessionid) = match (&session_state.sid, &session_state.gsessionid) {
                    (Some(sid), Some(gsessionid)) => (sid.as_str(), gsessionid.as_str()),
                    _ => {
                        warn!("Event task running without SID/gsessionid? Likely connect failed or disconnect occurred. Stopping task.");
                        // If connect() failed, this task shouldn't run long anyway.
                        // If disconnect() occurred, SID/gsessionid might be cleared.
                        break; // Exit loop if no session
                    }
                };
                let current_lounge_token = {
                    let state_guard = arcs.shared_state.read().await;
                    state_guard.lounge_token.clone()
                };
                let current_aid_val = arcs.aid_atomic.load(Ordering::SeqCst);
                let aid_string = current_aid_val.to_string();
                let params = [
                    ("SID", sid),
                    ("gsessionid", gsessionid),
                    ("RID", "rpc"),
                    ("VER", "8"),
                    ("v", "2"),
                    ("device", "REMOTE_CONTROL"),
                    ("app", "youtube-desktop"),
                    ("loungeIdToken", current_lounge_token.as_str()),
                    ("name", device_name.as_str()),
                    ("CI", "0"),
                    ("TYPE", "xmlhttp"),
                    ("AID", aid_string.as_str()),
                ];

                debug!(?params, "Sending event subscription request (long poll)");
                let response = match client
                    .get("https://www.youtube.com/api/lounge/bc/bind")
                    .query(&params)
                    .send()
                    .await
                {
                    Ok(res) => res,
                    Err(e) => {
                        // Handle errors *sending* the request itself (e.g., DNS, connection refused)
                        error!(error = %e, "Failed to send event subscription request");
                        sleep(Duration::from_secs(5)).await; // Backoff before retrying outer loop
                        continue; // Retry outer loop (send request again)
                    }
                };

                // --- Check Status Codes ---
                match response.status().as_u16() {
                    400 | 404 | 410 => {
                        let status = response.status(); // get it BEFORE .text()
                        let body_text = response.text().await.unwrap_or_default();
                        error!(
                            "Terminal HTTP status from server during event poll; disconnecting. Status: {}, Body: {}",
                            status,
                            body_text
                        );
                        let _ = event_sender.send(LoungeEvent::ScreenDisconnected);
                        break;
                    }
                    401 => {
                        warn!("Event task received 401 Unauthorized, attempting token refresh...");
                        match LoungeClient::refresh_lounge_token(&screen_id).await {
                            Ok(screen) => {
                                info!("Successfully refreshed token for screen_id: {}", screen_id);
                                {
                                    let mut state = arcs.shared_state.write().await;
                                    let old_token_preview =
                                        state.lounge_token.chars().take(8).collect::<String>();
                                    state.lounge_token = screen.lounge_token.clone();
                                    debug!(old = %old_token_preview, "Stored new lounge token in shared state.");
                                    if let Some(ref callback) = state.token_refresh_callback {
                                        debug!("Calling token refresh callback.");
                                        callback(&screen_id, &screen.lounge_token);
                                    } else {
                                        debug!("No token refresh callback set.");
                                    }
                                }
                                debug!("Retrying event subscription after token refresh.");
                                continue; // Retry the loop immediately
                            }
                            Err(refresh_err) => {
                                error!(error = %refresh_err, "Failed to refresh token after 401");
                                error!("Disconnecting event listener due to unrecoverable auth failure.");
                                let _ = event_sender.send(LoungeEvent::ScreenDisconnected);
                                break; // Stop the task loop
                            }
                        }
                    }
                    status if !response.status().is_success() => {
                        error!(status = %status, "Event subscription received unsuccessful status");
                        sleep(Duration::from_secs(10)).await; // Backoff
                        continue; // Retry loop
                    }
                    _ => {
                        // Success (2xx)
                        debug!(
                            "Event subscription request successful ({}), processing response.",
                            response.status()
                        );
                    }
                } // End status match

                // --- Process Streaming Response Body ---
                use futures::StreamExt;
                let mut stream = response.bytes_stream();
                let mut codec = LoungeCodec::new();
                let mut buffer = BytesMut::with_capacity(BUFFER_CAPACITY);

                'stream_loop: loop {
                    // Label this inner loop
                    // Wait for the next chunk OR the inactivity timeout
                    match timeout(INACTIVITY_TIMEOUT, stream.next()).await {
                        // --- Case 1: Data received within timeout ---
                        Ok(Some(Ok(chunk))) => {
                            // Data arrived! Resetting the implicit timer by proceeding.
                            if chunk.is_empty() {
                                // Handle empty chunk if necessary, maybe just ignore
                                debug!("Received empty chunk.");
                            } else {
                                buffer.extend_from_slice(&chunk);
                                loop {
                                    match codec.decode(&mut buffer) {
                                        Ok(Some(message)) => {
                                            process_event_chunk(
                                                &message,
                                                &event_sender,
                                                &arcs.latest_now_playing,
                                                &arcs.shared_state,
                                                &arcs.aid_atomic,
                                            )
                                            .await;
                                        }
                                        Ok(None) => break,
                                        Err(e) => {
                                            error!(error = %e, "Error decoding event message stream");
                                            buffer.clear();
                                            break 'stream_loop;
                                        }
                                    }
                                }
                                if buffer.is_empty() && buffer.capacity() > BUFFER_CAPACITY * 4 {
                                    debug!(
                                        "Buffer capacity ({}) exceeds threshold ({}), replacing with new buffer.",
                                        buffer.capacity(),
                                        BUFFER_CAPACITY * 4
                                    );
                                    buffer = BytesMut::with_capacity(BUFFER_CAPACITY);
                                }
                            }
                        }

                        // --- Case 2: Stream returned an error within timeout ---
                        Ok(Some(Err(e))) => {
                            error!(
                                err = %e,
                                cause = ?e.source(),
                                is_timeout = e.is_timeout(),
                                is_connect = e.is_connect(),
                                status = ?e.status(),
                                "Network/decode failure in event stream chunk, reconnecting."
                            );
                            break 'stream_loop;
                        }

                        // --- Case 3: Stream ended gracefully within timeout ---
                        Ok(None) => {
                            debug!(
                                "Event stream ended gracefully by server (EOF) -- will reconnect."
                            );
                            break 'stream_loop;
                        }

                        // --- Case 4: Inactivity Timeout expired ---
                        Err(_) => {
                            debug!(
                                "Inactivity detected (no data for {}s), reconnecting.",
                                INACTIVITY_TIMEOUT.as_secs()
                            );
                            break 'stream_loop;
                        }
                    }
                }
            }
        });
        Ok(()) // Return Ok: task was spawned successfully
    }

    /// Send a playback command to the screen
    pub async fn send_command(&mut self, command: PlaybackCommand) -> Result<(), LoungeError> {
        if !self.connected {
            warn!("Attempted to send command while not connected");
            return Err(LoungeError::ConnectionClosed);
        }
        let sid = self
            .session_state
            .sid
            .as_ref()
            .cloned()
            .ok_or(LoungeError::SessionExpired)?;
        let gsessionid = self
            .session_state
            .gsessionid
            .as_ref()
            .cloned()
            .ok_or(LoungeError::SessionExpired)?;

        // Update RID/offset specific to this main client instance's command sequence
        // These are local to `self.session_state`
        let rid = self.session_state.increment_rid();
        let ofs = self.session_state.increment_offset();

        let command_name = command.name();
        debug!(
            "Sending command: {} (RID: {}, offset: {})",
            command_name, rid, ofs
        );
        let token = {
            let state_guard = self.shared_state.read().await;
            state_guard.lounge_token.clone()
        };
        let mut form_fields: Vec<(&str, String)> = Vec::with_capacity(16);
        form_fields.push(("count", "1".to_string()));
        form_fields.push(("ofs", ofs.to_string()));
        form_fields.push(("req0__sc", command_name.to_string()));

        match &command {
            PlaybackCommand::SetPlaylist {
                video_id,
                list_id,
                current_index,
                current_time,
                audio_only,
                params,
                player_params,
            } => {
                form_fields.push(("req0_videoId", video_id.clone()));

                if let Some(idx) = current_index {
                    form_fields.push(("req0_currentIndex", idx.to_string()));
                }

                if let Some(list) = list_id {
                    form_fields.push(("req0_listId", list.clone()));
                }

                if let Some(time) = current_time {
                    form_fields.push(("req0_currentTime", time.to_string()));
                }

                if let Some(audio) = audio_only {
                    form_fields.push(("req0_audioOnly", audio.to_string()));
                }

                if let Some(p) = params {
                    form_fields.push(("req0_params", p.clone()));
                }

                if let Some(pp) = player_params {
                    form_fields.push(("req0_playerParams", pp.clone()));
                }

                form_fields.push((
                    "req0_prioritizeMobileSenderPlaybackStateOnConnection",
                    "true".to_string(),
                ));
            }

            PlaybackCommand::AddVideo {
                video_id,
                video_sources,
            } => {
                form_fields.push(("req0_videoId", video_id.clone()));
                if let Some(sources) = video_sources {
                    form_fields.push(("req0_videoSources", sources.clone()));
                }
            }

            PlaybackCommand::SeekTo { new_time } => {
                form_fields.push(("req0_newTime", new_time.to_string()));
            }

            PlaybackCommand::SetVolume { volume } => {
                form_fields.push(("req0_volume", volume.to_string()));
            }

            PlaybackCommand::SetAutoplayMode { autoplay_mode } => {
                form_fields.push(("req0_autoplayMode", autoplay_mode.clone()));
            }

            _ => {}
        }
        let current_aid = self.aid_atomic.load(Ordering::SeqCst);

        // Build query parameters
        let params = [
            ("SID", sid.as_str()),               // Use cloned SID
            ("gsessionid", gsessionid.as_str()), // Use cloned GSESSIONID
            ("RID", &rid.to_string()),           // Use main instance's RID
            ("VER", "8"),
            ("v", "2"),                        // v=2 common for commands
            ("TYPE", "bind"),                  // TYPE=bind common for commands
            ("t", "1"),                        // Common param, maybe timestamp related?
            ("AID", &current_aid.to_string()), // Use current_aid, maybe default to "0"
            ("CI", "0"),                       // Typically 0 for commands
            // Include device context for commands?
            ("name", self.device_name.as_str()),
            ("id", self.device_id.as_str()),
            ("device", "REMOTE_CONTROL"),
            ("loungeIdToken", token.as_str()), // Use cloned token
        ];

        debug!("Sending command request to YouTube Lounge API");

        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .form(&form_fields)
            .send()
            .await?;

        // Check status codes AFTER sending
        match response.status().as_u16() {
            400 => {
                warn!(
                    "Session likely expired (HTTP 400) when sending command: {}",
                    command_name
                );
                // Optionally try reading body for "REQUEST_INVALID" or similar
                self.connected = false; // Assume connection is dead
                let _ = self.event_sender.send(LoungeEvent::ScreenDisconnected); // Notify listeners
                return Err(LoungeError::SessionExpired);
            }
            401 => {
                warn!(
                    "Token expired (HTTP 401) when sending command: {}",
                    command_name
                );
                return Err(LoungeError::TokenExpired); // Specific error for refresh handling
            }
            404 => {
                // Session might be completely gone
                warn!(
                    "Session not found (HTTP 404) when sending command: {}",
                    command_name
                );
                self.connected = false;
                let _ = self.event_sender.send(LoungeEvent::ScreenDisconnected);
                return Err(LoungeError::SessionExpired); // Treat as session issue
            }
            410 => {
                warn!(
                    "Connection closed (HTTP 410) when sending command: {}",
                    command_name
                );
                self.connected = false;
                let _ = self.event_sender.send(LoungeEvent::ScreenDisconnected);
                return Err(LoungeError::ConnectionClosed);
            }
            status if !response.status().is_success() => {
                let body_text = response.text().await.unwrap_or_default();
                let error_msg = format!(
                    "Command '{}' failed with status {} and response body:\n{}",
                    command_name, status, body_text
                );
                error!("{}", error_msg);
                return Err(LoungeError::InvalidResponse(error_msg));
            }
            _ => {} // Success
        }

        debug!("Command sent successfully: {}", command_name);
        Ok(())
    }

    /// Send a command with automatic token refresh if needed
    pub async fn send_command_with_refresh(
        &mut self,
        command: PlaybackCommand,
    ) -> Result<(), LoungeError> {
        match self.send_command(command.clone()).await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                info!(
                    "Refreshing expired token (send_command_with_refresh for '{}')",
                    command.name()
                );
                let screen = Self::refresh_lounge_token(&self.screen_id).await?;
                {
                    let mut state = self.shared_state.write().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                }
                debug!("Retrying send_command for '{}'", command.name());
                self.send_command(command).await
            }
            Err(e) => Err(e),
        }
    }

    /// Disconnect from the screen properly
    pub async fn disconnect(&mut self) -> Result<(), LoungeError> {
        if !self.connected {
            debug!("Already disconnected or not connected, nothing to do.");
            return Ok(());
        }

        info!("Disconnecting from screen: {}", self.screen_id);

        // Mark as disconnected immediately, attempt cleanup best-effort
        self.connected = false;

        // Clone session details if they exist
        let sid = self.session_state.sid.clone();
        let gsessionid = self.session_state.gsessionid.clone();

        if let (Some(sid), Some(gsessionid)) = (sid, gsessionid) {
            // Use main instance's RID sequence for the terminate command
            let rid = self.session_state.increment_rid();

            let token = {
                let state_guard = self.shared_state.read().await;
                state_guard.lounge_token.clone()
            };
            // Prepare parameters for terminate request
            let params = [
                ("SID", sid.as_str()),
                ("gsessionid", gsessionid.as_str()),
                ("RID", &rid.to_string()), // RID for this request
                ("VER", "8"),
                ("v", "2"),
                ("TYPE", "terminate"), // Specific TYPE for disconnect
                ("loungeIdToken", token.as_str()), // Include token
                ("name", self.device_name.as_str()), // Include identity
                ("id", self.device_id.as_str()),
                ("device", "REMOTE_CONTROL"),
            ];

            // Form body for terminate (might just need TYPE in query params?)
            // Let's try with just query params first based on common patterns.
            // If needed: let form_data = "ui=&TYPE=terminate&clientDisconnectReason=MDX_SESSION_DISCONNECT_REASON_DISCONNECTED_BY_USER";

            debug!("Sending disconnect (terminate) request to YouTube Lounge API");

            // Send disconnect request - ignore errors, best effort
            let res = self
                .client
                .post("https://www.youtube.com/api/lounge/bc/bind")
                .query(&params)
                // .header("Content-Type", "application/x-www-form-urlencoded") // If using form body
                // .body(form_data) // If using form body
                .send()
                .await;

            if let Err(e) = res {
                warn!("Error sending disconnect request (ignored): {}", e);
            } else {
                debug!("Disconnect request sent.");
            }
        } else {
            warn!("No valid session details found, cannot send explicit terminate request.");
        }

        // Clear session state after attempting disconnect
        debug!("Clearing SessionState due to disconnect");
        self.session_state = SessionState::new();

        // Send disconnect event AFTER marking disconnected and attempting termination
        // Do this even if terminate fails, as the client considers itself disconnected.
        let _ = self.event_sender.send(LoungeEvent::ScreenDisconnected);

        // No sleep needed, just return
        info!("Client disconnected.");
        Ok(())
    }

    // Build form data for initial connection
    async fn build_connect_form_data(&self) -> Result<String, LoungeError> {
        let token = {
            let state_guard = self.shared_state.read().await;
            state_guard.lounge_token.clone()
        };
        let form_fields = [
            ("app", "web"),
            ("mdx-version", "3"),
            ("name", &self.device_name),
            ("id", &self.device_id),
            ("device", "REMOTE_CONTROL"),
            ("capabilities", "que,dsdtr,atp"),
            ("method", "setPlaylist"),
            ("magnaKey", "cloudPairedDevice"),
            ("ui", "false"),
            ("deviceContext", "user_agent=dunno"),
            ("window_width_points", ""),
            ("window_height_points", ""),
            ("os_name", "android"),
            ("ms", ""),
            ("theme", "cl"),
            ("loungeIdToken", token.as_str()), // Use cloned token
        ];

        serde_urlencoded::to_string(form_fields).map_err(LoungeError::UrlEncodingFailed)
    }

    // Get video thumbnail URL
    pub fn get_thumbnail_url(video_id: &str, thumbnail_idx: u8) -> String {
        format!(
            "https://img.youtube.com/vi/{}/{}.jpg",
            video_id, thumbnail_idx
        )
    }

    pub async fn play(&mut self) -> Result<(), LoungeError> {
        info!("Sending Play command");
        self.send_command_with_refresh(PlaybackCommand::Play).await
    }

    pub async fn pause(&mut self) -> Result<(), LoungeError> {
        info!("Sending Pause command");
        self.send_command_with_refresh(PlaybackCommand::Pause).await
    }

    pub async fn next(&mut self) -> Result<(), LoungeError> {
        info!("Sending Next command");
        self.send_command_with_refresh(PlaybackCommand::Next).await
    }

    pub async fn previous(&mut self) -> Result<(), LoungeError> {
        info!("Sending Previous command");
        self.send_command_with_refresh(PlaybackCommand::Previous)
            .await
    }

    pub async fn skip_ad(&mut self) -> Result<(), LoungeError> {
        info!("Sending SkipAd command");
        self.send_command_with_refresh(PlaybackCommand::SkipAd)
            .await
    }

    pub async fn mute(&mut self) -> Result<(), LoungeError> {
        info!("Sending Mute command");
        self.send_command_with_refresh(PlaybackCommand::Mute).await
    }

    pub async fn unmute(&mut self) -> Result<(), LoungeError> {
        info!("Sending Unmute command");
        self.send_command_with_refresh(PlaybackCommand::Unmute)
            .await
    }

    pub async fn seek_to(&mut self, new_time: f64) -> Result<(), LoungeError> {
        info!(seek_time = new_time, "Sending SeekTo command");
        self.send_command_with_refresh(PlaybackCommand::SeekTo { new_time })
            .await
    }

    pub async fn set_volume(&mut self, volume: i32) -> Result<(), LoungeError> {
        info!(volume_level = volume, "Sending SetVolume command");
        self.send_command_with_refresh(PlaybackCommand::SetVolume { volume })
            .await
    }

    pub async fn set_autoplay_mode(&mut self, autoplay_mode: String) -> Result<(), LoungeError> {
        info!(mode = %autoplay_mode, "Sending SetAutoplayMode command");
        self.send_command_with_refresh(PlaybackCommand::SetAutoplayMode { autoplay_mode })
            .await
    }

    pub async fn play_video(&mut self, video_id: String) -> Result<(), LoungeError> {
        info!(video_id = %video_id, "Sending SetPlaylist command (single video)");
        self.send_command_with_refresh(PlaybackCommand::set_playlist(video_id))
            .await
    }

    pub async fn add_video_to_queue(&mut self, video_id: String) -> Result<(), LoungeError> {
        info!(video_id = %video_id, "Sending AddVideo command");
        self.send_command_with_refresh(PlaybackCommand::add_video(video_id))
            .await
    }

    pub async fn play_playlist(&mut self, list_id: String) -> Result<(), LoungeError> {
        info!(list_id = %list_id, "Sending SetPlaylist command (by list ID)");
        self.send_command_with_refresh(PlaybackCommand::set_playlist_by_id(list_id))
            .await
    }

    pub async fn play_playlist_at_index(
        &mut self,
        list_id: String,
        index: i32,
    ) -> Result<(), LoungeError> {
        info!(list_id = %list_id, index = index, "Sending SetPlaylist command (by list ID and index)");
        self.send_command_with_refresh(PlaybackCommand::set_playlist_with_index(list_id, index))
            .await
    }
}

lazy_static! {
    static ref SID_RE: Regex = Regex::new(r#"\["c","([^"]*)""#).unwrap();
    static ref GSESSIONID_RE: Regex = Regex::new(r#"\["S","([^"]*)""#).unwrap();
}
fn extract_session_ids(body: &[u8]) -> Result<(Option<String>, Option<String>), LoungeError> {
    let full_response = String::from_utf8_lossy(body);
    let sid = SID_RE
        .captures(&full_response)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()));
    let gsessionid = GSESSIONID_RE
        .captures(&full_response)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()));
    match (sid, gsessionid) {
        (Some(sid), Some(gsessionid)) => Ok((Some(sid), Some(gsessionid))),
        _ => Err(LoungeError::InvalidResponse(
            "Failed to obtain session IDs".to_string(),
        )),
    }
}

async fn process_event_chunk(
    chunk: &str,
    sender: &broadcast::Sender<LoungeEvent>,
    latest_now_playing_arc: &Arc<RwLock<Option<NowPlaying>>>,
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
                            deserialize_with_logging::<PlaybackState>(event_type, payload)
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
                            deserialize_with_logging::<NowPlaying>(event_type, payload)
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
                                        let state = PlaybackState {
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
                                    let state_from_np = PlaybackState {
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
                            deserialize_with_logging::<LoungeStatus>(event_type, payload)
                        {
                            match serde_json::from_str::<Vec<Device>>(&status.devices) {
                                Ok(devices) => {
                                    // Process devices (parse device_info if available)
                                    let devices_with_info: Vec<Device> = devices
                                        .into_iter()
                                        .map(|mut device| {
                                            if !device.device_info_raw.trim().is_empty() {
                                                match serde_json::from_str::<DeviceInfo>(
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
                        if let Ok(state) = deserialize_with_logging::<AdState>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::AdStateChange(state));
                        }
                    }
                    "onSubtitlesTrackChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<SubtitlesTrackChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::SubtitlesTrackChanged(state));
                        }
                    }
                    "onAudioTrackChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<AudioTrackChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::AudioTrackChanged(state));
                        }
                    }
                    "onAutoplayModeChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<AutoplayModeChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::AutoplayModeChanged(state));
                        }
                    }
                    "onHasPreviousNextChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<HasPreviousNextChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::HasPreviousNextChanged(state));
                        }
                    }
                    "onVideoQualityChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<VideoQualityChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::VideoQualityChanged(state));
                        }
                    }
                    "onVolumeChanged" => {
                        if let Ok(state) =
                            deserialize_with_logging::<VolumeChanged>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::VolumeChanged(state));
                        }
                    }
                    "playlistModified" => {
                        if let Ok(state) =
                            deserialize_with_logging::<PlaylistModified>(event_type, payload)
                        {
                            let _ = sender.send(LoungeEvent::PlaylistModified(state));
                        }
                    }
                    "autoplayUpNext" => {
                        if let Ok(state) =
                            deserialize_with_logging::<AutoplayUpNext>(event_type, payload)
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

// Helper module for parsing YouTube's string values
pub mod youtube_parse {
    #[allow(dead_code)]
    pub fn parse_float(s: &str) -> f64 {
        s.parse::<f64>().unwrap_or(0.0)
    }

    pub fn parse_int(s: &str) -> i32 {
        s.parse::<i32>().unwrap_or(0)
    }

    pub fn parse_bool(s: &str) -> bool {
        s == "true"
    }

    pub fn parse_list(s: &str) -> Vec<String> {
        s.split(',').map(|s| s.trim().to_string()).collect()
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

// Helper methods for AdState
impl AdState {
    pub fn is_skippable(&self) -> bool {
        self.is_skip_enabled
    }

    pub fn get_content_video_id(&self) -> &str {
        &self.content_video_id
    }
}

// Safety traits
impl std::fmt::Debug for LoungeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoungeClient")
            .field("device_id", &self.device_id)
            .field("screen_id", &self.screen_id)
            .field("device_name", &self.device_name)
            .field("connected", &self.connected)
            .finish()
    }
}
