mod codec;
pub use codec::LoungeCodec;
mod commands;
pub use commands::PlaybackCommand;
mod error;
pub use error::LoungeError;
mod events;
pub use events::{LoungeEvent, PlaybackSession, PlaybackStatus};
mod models;
pub use models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, Device, DeviceInfo,
    HasPreviousNextChanged, LoungeStatus, NowPlaying, PlaybackState, PlaylistModified, Screen,
    ScreenResponse, ScreensResponse, SubtitlesTrackChanged, VideoData, VideoQualityChanged,
    VolumeChanged,
};
mod state;
use state::{InnerState, SessionState};
mod utils;
pub use utils::youtube_parse;

use bytes::BytesMut;
use reqwest::Client;
use std::error::Error;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{sleep, timeout, Duration};
use tokio_util::codec::Decoder;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const BUFFER_CAPACITY: usize = 16 * 1024; // 16KB initial buffer capacity
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(32); // Wait 32s for next chunk

// Type alias for the optional callback function pointer for clarity
pub type TokenCallback = Option<Box<dyn Fn(&str, &str) + Send + Sync + 'static>>;

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
/// - `TRACE`: Shows all logs, including detailed internal operations
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
        let (sid, gsessionid) = crate::utils::extract_session_ids(&body)?;

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
                                            events::process_event_chunk(
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

    pub fn get_thumbnail_url(video_id: &str, thumbnail_idx: u8) -> String {
        format!(
            "https://img.youtube.com/vi/{}/{}.jpg",
            video_id, thumbnail_idx
        )
    }

    pub async fn play(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Play).await
    }

    pub async fn pause(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Pause).await
    }

    pub async fn next(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Next).await
    }

    pub async fn previous(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Previous)
            .await
    }

    pub async fn skip_ad(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SkipAd)
            .await
    }

    pub async fn mute(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Mute).await
    }

    pub async fn unmute(&mut self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Unmute)
            .await
    }

    pub async fn seek_to(&mut self, new_time: f64) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SeekTo { new_time })
            .await
    }

    pub async fn set_volume(&mut self, volume: i32) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SetVolume { volume })
            .await
    }

    pub async fn set_autoplay_mode(&mut self, autoplay_mode: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SetAutoplayMode { autoplay_mode })
            .await
    }

    pub async fn play_video(&mut self, video_id: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::set_playlist(video_id))
            .await
    }

    pub async fn add_video_to_queue(&mut self, video_id: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::add_video(video_id))
            .await
    }

    pub async fn play_playlist(&mut self, list_id: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::set_playlist_by_id(list_id))
            .await
    }

    pub async fn play_playlist_at_index(
        &mut self,
        list_id: String,
        index: i32,
    ) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::set_playlist_with_index(list_id, index))
            .await
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
