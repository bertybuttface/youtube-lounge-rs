use bytes::BytesMut;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};
use tokio_util::codec::Decoder;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const BUFFER_CAPACITY: usize = 16 * 1024; // 16KB initial buffer capacity
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(32); // Wait 32s for next chunk

// Type alias for the optional callback function pointer for clarity
type TokenCallback = Option<Box<dyn Fn(&str, &str) + Send + Sync + 'static>>;

// Struct to hold the state that needs to be shared and mutated async
// #[derive(Debug)] // Add Debug for easier inspection if needed (will need to fix errors)
struct InnerState {
    lounge_token: String,
    token_refresh_callback: TokenCallback,
    aid: Option<String>,
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

// Models

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
    LoungeStatus(Vec<Device>, Option<String>), // Now includes queue_id
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
    rid: i32,
    command_offset: i32,
}

impl SessionState {
    fn new() -> Self {
        Self {
            sid: None,
            gsessionid: None,
            rid: 1,
            command_offset: 0,
        }
    }

    fn increment_rid(&mut self) -> i32 {
        self.rid += 1;
        self.rid
    }

    fn increment_offset(&mut self) -> i32 {
        self.command_offset += 1;
        self.command_offset
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
    client: Client,
    device_id: String,
    screen_id: String,
    device_name: String,
    session_state: SessionState,
    event_sender: broadcast::Sender<LoungeEvent>,
    connected: bool,
    // Track latest NowPlaying with CPN for PlaybackSession generation
    latest_now_playing: Arc<Mutex<Option<NowPlaying>>>, // Wrap in Arc<Mutex>
    shared_state: Arc<Mutex<InnerState>>,               // Add the shared state holder
}

impl LoungeClient {
    /// Create a new LoungeClient. If a device_id is provided, it will be used;
    /// otherwise, a new UUID is generated.
    pub fn new(
        screen_id: &str,
        lounge_token: &str,
        device_name: &str,
        device_id: Option<&str>,
    ) -> Self {
        let client = Client::new();
        let device_id = device_id.map_or_else(|| Uuid::new_v4().to_string(), ToString::to_string);
        let (event_tx, _) = broadcast::channel(100);

        // Initialize the inner state for the Mutex
        let initial_state = InnerState {
            lounge_token: lounge_token.to_string(),
            token_refresh_callback: None, // Will be set later via method
            aid: None,
        };

        Self {
            client,
            device_id,
            screen_id: screen_id.to_string(),
            device_name: device_name.to_string(),
            session_state: SessionState::new(),
            event_sender: event_tx,
            connected: false,
            latest_now_playing: Arc::new(Mutex::new(None)), // Initialize Arc<Mutex>
            shared_state: Arc::new(Mutex::new(initial_state)), // Initialize Arc<Mutex>
        }
    }

    pub async fn set_token_refresh_callback<F>(&self, callback: F)
    // Now takes &self, is async
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        // Lock the shared state to update the callback
        let mut state_guard = self.shared_state.lock().await;
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
            let error_msg = format!("Failed to refresh token: {}", response.status());
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

        // --- Lock, clone token, unlock ---
        let token = {
            let state_guard = self.shared_state.lock().await;
            state_guard.lounge_token.clone()
        }; // Lock released

        let params = [("lounge_token", &token)]; // Use cloned token

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
                // --- Lock, Update Token, Call Callback ---
                {
                    let mut state = self.shared_state.lock().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                } // Lock released
                debug!("Retrying check_screen_availability");
                self.check_screen_availability().await
            }
            Err(e) => Err(e),
        }
    }

    /// Connect to the screen and establish a session
    pub async fn connect(&mut self) -> Result<(), LoungeError> {
        info!("Connecting to screen: {}", self.screen_id);

        // Reset session state
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
                let error_msg = format!("Failed to connect: {}", response.status());
                error!("{}", error_msg);
                debug!("Response status: {}", status);
                // Optionally read body for more details if available
                let body_text = response.text().await.unwrap_or_default();
                error!("Response body: {}", body_text);
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

        // Send session established event
        if self.event_sender.receiver_count() > 0 {
            debug!(
                "Sending SessionEstablished event to {} receiver(s)",
                self.event_sender.receiver_count()
            );
            let _ = self.event_sender.send(LoungeEvent::SessionEstablished);
        }

        // Start event subscription in background
        info!("Starting event subscription task");
        self.subscribe_to_events().await?;

        info!("Successfully connected to screen: {}", self.screen_id);
        Ok(())
    }

    /// Connect to the screen with automatic token refresh if needed
    pub async fn connect_with_refresh(&mut self) -> Result<(), LoungeError> {
        match self.connect().await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                info!("Refreshing expired token (connect_with_refresh)");
                let screen = Self::refresh_lounge_token(&self.screen_id).await?;
                // --- Lock, Update Token, Call Callback ---
                {
                    let mut state = self.shared_state.lock().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                } // Lock released
                debug!("Retrying connect");
                self.connect().await
            }
            Err(e) => Err(e),
        }
    }

    async fn subscribe_to_events(&self) -> Result<(), LoungeError> {
        let client = self.client.clone();
        let device_name = self.device_name.clone();
        let screen_id = self.screen_id.clone();
        let event_sender = self.event_sender.clone();
        // Clone session state for the task to manage its AID/RID/offset independently
        let session_state = self.session_state.clone(); // Task manages its own RID/offset copy

        // --- Clone Arcs for the task ---
        let latest_now_playing_arc = self.latest_now_playing.clone();
        let shared_state_arc = self.shared_state.clone();
        // --- End Arc cloning ---

        tokio::spawn(async move {
            // Move cloned Arcs into the task
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

                // --- Lock, clone token, unlock (inside loop for potential updates) ---
                let current_lounge_token = {
                    let state_guard = shared_state_arc.lock().await;
                    state_guard.lounge_token.clone()
                }; // Lock released

                // Build parameters for the event bind request
                let mut params = HashMap::new();
                params.insert("SID", sid);
                params.insert("gsessionid", gsessionid);
                params.insert("RID", "rpc"); // Use "rpc" for event channel
                params.insert("VER", "8");
                params.insert("v", "2");
                params.insert("device", "REMOTE_CONTROL");
                params.insert("app", "youtube-desktop"); // Or "web"? Check captures.
                params.insert("loungeIdToken", current_lounge_token.as_str()); // Use current token
                params.insert("name", device_name.as_str());
                params.insert("CI", "0"); // Use 0 for long polling start
                params.insert("TYPE", "xmlhttp"); // Standard for browser long-polling

                // Add AID if task's session state has received one
                // --- Lock to READ aid ---
                let current_aid_opt: Option<String> = {
                    let guard = shared_state_arc.lock().await;
                    guard.aid.clone()
                }; // Lock released

                // --- Get &str using as_deref or default ---
                // .as_deref() converts Option<String> -> Option<&str> by deref-coercing String to &str
                let aid_value_to_insert: &str = current_aid_opt.as_deref().unwrap_or("0");

                // --- Insert the valid &str into params ---
                params.insert("AID", aid_value_to_insert);

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
                        error!(
                            "Event task received terminal status ({}), disconnecting.",
                            response.status()
                        );
                        let _ = event_sender.send(LoungeEvent::ScreenDisconnected);
                        break; // Stop the task loop
                    }
                    401 => {
                        warn!("Event task received 401 Unauthorized, attempting token refresh...");
                        match LoungeClient::refresh_lounge_token(&screen_id).await {
                            Ok(screen) => {
                                info!("Successfully refreshed token for screen_id: {}", screen_id);
                                // --- Lock Shared State, Update Token, Call Callback ---
                                {
                                    // Scope for lock guard
                                    let mut state = shared_state_arc.lock().await;
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
                                } // Lock released here
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
                                // continue 'stream_loop; // Or potentially break if it signals end? Test behavior.
                            }

                            // Add data to buffer
                            buffer.extend_from_slice(&chunk);

                            // Decode and process all available messages in the buffer
                            loop {
                                // Inner decode loop
                                match codec.decode(&mut buffer) {
                                    Ok(Some(message)) => {
                                        // Successfully decoded a message, process it
                                        // (This now includes handling JSON noop messages gracefully)
                                        process_event_chunk(
                                            &message,
                                            &event_sender,
                                            &latest_now_playing_arc,
                                            &shared_state_arc,
                                        )
                                        .await;
                                    }
                                    Ok(None) => {
                                        // Codec needs more data, break decode loop and wait for next chunk
                                        break;
                                    }
                                    Err(e) => {
                                        // Decoding error, likely corrupt data
                                        error!(error = %e, "Error decoding event message stream");
                                        buffer.clear(); // Clear potentially bad data
                                                        // Decide whether to break inner decode loop or outer stream loop.
                                                        // Breaking outer seems safer to force a reconnect on decode errors.
                                        break 'stream_loop;
                                    }
                                }
                            } // End inner decode loop

                            // If the buffer is now empty BUT its capacity is much larger than needed,
                            // replace it with a new one to reclaim memory.
                            // This is less aggressive than shrinking but avoids holding huge unused allocations.
                            if buffer.is_empty() && buffer.capacity() > BUFFER_CAPACITY * 4 {
                                debug!(
                                    "Buffer capacity ({}) exceeds threshold ({}), replacing with new buffer.",
                                    buffer.capacity(),
                                    BUFFER_CAPACITY * 4
                                );
                                // Replace the large buffer with a fresh one
                                buffer = BytesMut::with_capacity(BUFFER_CAPACITY);
                            }
                            // If we successfully processed the chunk, the 'stream_loop continues,
                            // effectively waiting for the next chunk with a fresh timeout.
                        }

                        // --- Case 2: Stream returned an error within timeout ---
                        Ok(Some(Err(e))) => {
                            // An error occurred while reading the stream (e.g., network issue, connection reset)
                            error!(error = %e, "Error reading event stream chunk");
                            break 'stream_loop; // Break inner loop to force reconnect via outer loop
                        }

                        // --- Case 3: Stream ended gracefully within timeout ---
                        Ok(None) => {
                            // The server closed the connection cleanly.
                            debug!("Event stream ended gracefully by server.");
                            break 'stream_loop; // Break inner loop to force reconnect via outer loop
                        }

                        // --- Case 4: `tokio::time::timeout` expired ---
                        Err(_) => {
                            // Inactivity detected! No data received for INACTIVITY_TIMEOUT duration.
                            debug!(
                                "Inactivity detected (no data for {}s), reconnecting.",
                                INACTIVITY_TIMEOUT.as_secs()
                            );
                            break 'stream_loop; // Break inner loop to force reconnect via outer loop
                        }
                    } // End match timeout result
                } // End 'stream_loop

                // This point is reached if 'stream_loop' was broken (due to error, stream end, or inactivity timeout)
                debug!("Event stream poll cycle finished, reconnecting for next poll...");
                // The outer 'loop' will automatically continue, initiating a new connection attempt.
                // Optional short sleep before reconnecting? Usually not necessary unless hammering the server.
                // sleep(Duration::from_millis(100)).await;
            } // End main task loop

            warn!("Event subscriber task finished.");
        });
        Ok(()) // Return Ok: task was spawned successfully
    }

    /// Send a playback command to the screen
    pub async fn send_command(&mut self, command: PlaybackCommand) -> Result<(), LoungeError> {
        if !self.connected {
            warn!("Attempted to send command while not connected");
            return Err(LoungeError::ConnectionClosed);
        }

        // Clone session details needed for request
        // Use ok_or to convert Option to Result early
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

        // --- Lock, clone token, unlock ---
        let token = {
            let state_guard = self.shared_state.lock().await;
            state_guard.lounge_token.clone()
        }; // Lock released

        // Form body as Vec of (&str, String) for reqwest .form()
        let mut form_fields: Vec<(&str, String)> = vec![
            ("count", "1".to_string()),
            ("ofs", ofs.to_string()), // Use main instance's offset
            ("req0__sc", command_name.to_string()),
        ];

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

        // --- Lock to READ aid ---
        let current_aid = {
            let guard = self.shared_state.lock().await;
            guard.aid.clone()
        }; // Lock released

        // Build query parameters
        let params = [
            ("SID", sid.as_str()),               // Use cloned SID
            ("gsessionid", gsessionid.as_str()), // Use cloned GSESSIONID
            ("RID", &rid.to_string()),           // Use main instance's RID
            ("VER", "8"),
            ("v", "2"),                                     // v=2 common for commands
            ("TYPE", "bind"),                               // TYPE=bind common for commands
            ("t", "1"), // Common param, maybe timestamp related?
            ("AID", current_aid.as_deref().unwrap_or("0")), // Use current_aid, maybe default to "0"
            ("CI", "0"), // Typically 0 for commands
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
            .form(&form_fields) // 💡 magic happens here — safe, clean, escaped
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
                let error_msg = format!("Command failed: {}", response.status());
                error!("{} for command: {}", error_msg, command_name);
                debug!("Response status: {}", status);
                // Optionally read body
                let body_text = response.text().await.unwrap_or_default();
                error!("Response body: {}", body_text);
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
                // --- Lock, Update Token, Call Callback ---
                {
                    let mut state = self.shared_state.lock().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                } // Lock released
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

            // --- Lock, clone token, unlock ---
            let token = {
                let state_guard = self.shared_state.lock().await;
                state_guard.lounge_token.clone()
            }; // Lock released

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
        // --- Lock, clone token, unlock ---
        let token = {
            let state_guard = self.shared_state.lock().await;
            state_guard.lounge_token.clone()
        }; // Lock released

        let form_fields = vec![
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

        serde_urlencoded::to_string(&form_fields).map_err(LoungeError::UrlEncodingFailed)
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

// Helper function to extract session IDs
fn extract_session_ids(body: &[u8]) -> Result<(Option<String>, Option<String>), LoungeError> {
    let full_response = String::from_utf8_lossy(body);

    // Helper function to extract value between markers
    fn extract_value(text: &str, marker: &str) -> Option<String> {
        text.find(marker).and_then(|idx| {
            let start = idx + marker.len();
            text[start..]
                .find('\"')
                .map(|end_idx| text[start..start + end_idx].to_string())
        })
    }

    // Extract sid and gsessionid
    let sid = extract_value(&full_response, "[\"c\",\"");
    let gsessionid = extract_value(&full_response, "[\"S\",\"");

    // Check if we found the session IDs using pattern matching
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
    latest_now_playing_arc: &Arc<Mutex<Option<NowPlaying>>>, // Accept Arc ref
    shared_state_arc: &Arc<Mutex<InnerState>>,
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
            // --- Lock to WRITE aid ---
            let mut state_guard = shared_state_arc.lock().await;
            state_guard.aid = Some(event_id.to_string());
            // --- Lock released ---
        }

        if let Some(event_array) = event.get(1).and_then(|v| v.as_array()) {
            // Check for the specific JSON noop structure [[N, ["noop"]]]
            if event_array.len() == 1 {
                // Should only contain ["noop"]
                if let Some(event_type) = event_array.first().and_then(|t| t.as_str()) {
                    if event_type == "noop" {
                        debug!("Received JSON noop event, connection alive.");
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
                            // --- Lock to READ latest_now_playing ---
                            let latest_np = {
                                // Scope for read lock
                                let guard = latest_now_playing_arc.lock().await;
                                guard.clone() // Clone Option<NowPlaying>
                            }; // Read lock released
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

                            // --- Lock to WRITE latest_now_playing ---
                            if now_playing.cpn.is_some() {
                                let mut guard = latest_now_playing_arc.lock().await;
                                *guard = Some(now_playing.clone());
                            } // Write lock released

                            // Create and send a PlaybackSession if possible
                            match now_playing.state.as_str() {
                                // Handle stop events (-1)
                                "-1" if now_playing.video_id.is_empty() => {
                                    // --- Lock to READ latest_now_playing_arc ---
                                    let prev_np_opt = {
                                        // Scope for read lock
                                        let guard = latest_now_playing_arc.lock().await;
                                        guard.clone() // Clone Option<NowPlaying>
                                    }; // Read lock released
                                       // --- Use the cloned value ---
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
