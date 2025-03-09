use bytes::Bytes;
use futures::StreamExt;
use once_cell::sync::Lazy;
use reqwest::Response;
use reqwest::{Client, ClientBuilder};
use serde_json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task;
use tokio::time::sleep;

// Import the debug_log macro from our utils module
use crate::debug_log;

// Connection constants
const STANDARD_REQUEST_TIMEOUT: u64 = 300; // 5 minute timeout
const LONG_POLL_TIMEOUT: u64 = 32 * 60; // 32 minutes timeout
const POOL_IDLE_TIMEOUT: u64 = 90; // 90 seconds idle timeout
const RECONNECT_DELAY: u64 = 5; // 5 second delay before retrying
const RECONNECT_SHORT_DELAY: u64 = 1; // 1 second delay for normal reconnection

// Create a static shared HTTP client for better connection pooling and DNS caching
static SHARED_CLIENT: Lazy<Client> = Lazy::new(|| {
    ClientBuilder::new()
        .timeout(Duration::from_secs(STANDARD_REQUEST_TIMEOUT))
        .pool_idle_timeout(Some(Duration::from_secs(POOL_IDLE_TIMEOUT)))
        .build()
        .expect("Failed to create shared HTTP client")
});

// Create a separate client for long polling with a timeout slightly longer than the heartbeat interval
// The YouTube API should send at least a NOOP message every 30 minutes
static LONG_POLL_CLIENT: Lazy<Client> = Lazy::new(|| {
    ClientBuilder::new()
        .timeout(Duration::from_secs(LONG_POLL_TIMEOUT))
        .pool_idle_timeout(Some(Duration::from_secs(POOL_IDLE_TIMEOUT)))
        .build()
        .expect("Failed to create long poll HTTP client")
});

/// Helper struct to handle HTTP responses and common error cases
struct HttpResponseHandler<'a> {
    connected: &'a Arc<Mutex<bool>>,
    event_sender: &'a broadcast::Sender<LoungeEvent>,
}

impl<'a> HttpResponseHandler<'a> {
    /// Create a new HTTP response handler
    fn new(
        connected: &'a Arc<Mutex<bool>>,
        event_sender: &'a broadcast::Sender<LoungeEvent>,
    ) -> Self {
        Self {
            connected,
            event_sender,
        }
    }

    /// Helper method to send events - logs send errors
    #[inline]
    fn send_event(&self, event: LoungeEvent) {
        if let Err(err) = self.event_sender.send(event.clone()) {
            eprintln!("Failed to broadcast {} event: {}", event.name(), err);
        }
    }

    /// Handle standard YouTube API error status codes
    fn handle_error_status(&self, status: u16) -> Option<LoungeError> {
        // Define error mapping and handle common cases
        let specific_error = match status {
            400 => Some(LoungeError::SessionExpired),
            401 => Some(LoungeError::TokenExpired),
            410 => Some(LoungeError::ConnectionClosed),
            _ => None,
        };

        // For specific errors, mark as disconnected and notify
        if let Some(error) = specific_error {
            match self.connected.lock() {
                Ok(mut lock) => *lock = false,
                Err(err) => eprintln!("Mutex poisoned when updating connected state: {:?}", err),
            }
            self.send_event(LoungeEvent::ScreenDisconnected);
            return Some(error);
        }

        // Handle other error statuses
        if status >= 400 {
            return Some(LoungeError::InvalidResponse(format!(
                "HTTP error status: {}",
                status
            )));
        }

        None
    }

    /// Process a response and return an error if the status code indicates an error
    fn check_response(&self, response: &Response) -> Result<(), LoungeError> {
        let status = response.status().as_u16();
        if let Some(error) = self.handle_error_status(status) {
            return Err(error);
        }
        Ok(())
    }
}

use crate::commands::{get_command_name, PlaybackCommand};
use crate::error::LoungeError;
use crate::events::LoungeEvent;
use crate::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, Device, DeviceInfo,
    HasPreviousNextChanged, LoungeStatus, NowPlaying, PlaybackSession, PlaybackState,
    PlaylistModified, Screen, ScreenAvailabilityResponse, ScreenResponse, ScreensResponse,
    SubtitlesTrackChanged, VideoData, VideoQualityChanged, VolumeChanged,
};
use crate::session::PlaybackSessionManager;

// Session state
#[derive(Debug, Clone)]
struct SessionState {
    sid: Option<String>,
    gsessionid: Option<String>,
    aid: Option<String>,
    rid: i32,
    command_offset: i32,
    // Flag to enable debug mode
    debug_mode: bool,
}

impl SessionState {
    const fn new() -> Self {
        Self {
            sid: None,
            gsessionid: None,
            aid: None,
            rid: 1,
            command_offset: 0,
            debug_mode: false,
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

    /// Get the current session information, useful before async operations
    fn get_session_info(&self) -> (Option<String>, Option<String>, Option<String>) {
        (self.sid.clone(), self.gsessionid.clone(), self.aid.clone())
    }

    /// Check if the session is valid (has sid and gsessionid)
    fn is_valid(&self) -> bool {
        self.sid.is_some() && self.gsessionid.is_some()
    }
}

// Type for token refresh callback function
pub type TokenRefreshCallback = Box<dyn Fn(&str, &str) + Send + Sync>;

// YouTube Lounge API client
pub struct LoungeClient {
    client: Client,
    screen_id: String,
    lounge_token: String,
    device_name: String,
    session_state: Arc<Mutex<SessionState>>,
    event_sender: broadcast::Sender<LoungeEvent>,
    connected: Arc<Mutex<bool>>,
    token_refresh_callback: Option<TokenRefreshCallback>,
    // Session manager handles all session tracking
    session_manager: PlaybackSessionManager,
}

impl LoungeClient {
    pub fn new(screen_id: &str, lounge_token: &str, device_name: &str) -> Self {
        // Use the shared client to benefit from connection pooling and DNS caching
        let client = SHARED_CLIENT.clone();

        // Create a broadcast channel with capacity for 100 events
        let (event_tx, _event_rx) = broadcast::channel(100);

        // Create a new session manager
        let session_manager = PlaybackSessionManager::new();

        Self {
            client,
            screen_id: screen_id.to_string(),
            lounge_token: lounge_token.to_string(),
            device_name: device_name.to_string(),
            session_state: Arc::new(Mutex::new(SessionState::new())),
            event_sender: event_tx,
            connected: Arc::new(Mutex::new(false)),
            token_refresh_callback: None,
            session_manager,
        }
    }

    /// Set a callback function that will be called when the token is refreshed
    /// The callback will receive the screen_id and the new lounge_token as parameters
    pub fn set_token_refresh_callback<F>(&mut self, callback: F)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        self.token_refresh_callback = Some(Box::new(callback));
    }

    /// Enable debug mode to get raw JSON payloads with events
    pub fn enable_debug_mode(&mut self) {
        match self.session_state.lock() {
            Ok(mut state) => {
                state.debug_mode = true;
                self.session_manager.enable_debug_mode();
            }
            Err(err) => eprintln!("Failed to enable debug mode - mutex poisoned: {:?}", err),
        }
    }

    /// Disable debug mode
    pub fn disable_debug_mode(&mut self) {
        match self.session_state.lock() {
            Ok(mut state) => {
                state.debug_mode = false;
                self.session_manager.disable_debug_mode();
            }
            Err(err) => eprintln!("Failed to disable debug mode - mutex poisoned: {:?}", err),
        }
    }

    /// Internal method to refresh the token
    async fn refresh_token_internal(&mut self) -> Result<(), LoungeError> {
        // Call the static refresh method
        let screen = Self::refresh_lounge_token(&self.screen_id).await?;

        // Update the client's token
        self.lounge_token.clone_from(&screen.lounge_token);

        // Call the callback if it exists
        if let Some(callback) = &self.token_refresh_callback {
            (callback)(&self.screen_id, &self.lounge_token);
        }

        Ok(())
    }

    // Get the event receiver for listening to events
    // This returns a broadcast::Receiver directly, eliminating the extra channel hop
    pub fn event_receiver(&self) -> broadcast::Receiver<LoungeEvent> {
        // Subscribe directly to the broadcast channel
        // This avoids the overhead of spawning a task and forwarding messages
        self.event_sender.subscribe()
    }

    // Get the session receiver for listening to playback session updates
    pub fn session_receiver(&self) -> broadcast::Receiver<PlaybackSession> {
        self.session_manager.subscribe()
    }

    // Get session by CPN
    pub fn get_session_by_cpn(&self, cpn: &str) -> Option<PlaybackSession> {
        self.session_manager.get_session_by_cpn(cpn)
    }

    // Get session by device ID through list_id mapping
    pub fn get_session_for_device(&self, device_id: &str) -> Option<PlaybackSession> {
        self.session_manager.get_session_for_device(device_id)
    }

    // Get most recent session
    pub fn get_current_session(&self) -> Option<PlaybackSession> {
        self.session_manager.get_current_session()
    }

    // Get all active sessions
    pub fn get_all_sessions(&self) -> Vec<PlaybackSession> {
        self.session_manager.get_all_sessions()
    }

    // Get currently playing sessions
    pub fn get_playing_sessions(&self) -> Vec<PlaybackSession> {
        self.session_manager.get_playing_sessions()
    }

    // Pair with a screen using a pairing code
    pub async fn pair_with_screen(pairing_code: &str) -> Result<Screen, LoungeError> {
        // Use shared client for better connection pooling
        let params = [("pairing_code", pairing_code)];

        let response = SHARED_CLIENT
            .post("https://www.youtube.com/api/lounge/pairing/get_screen")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(LoungeError::InvalidResponse(format!(
                "Failed to pair with screen: {}",
                response.status()
            )));
        }

        let screen_response = response.json::<ScreenResponse>().await?;
        Ok(screen_response.screen)
    }

    // Refresh the lounge token
    pub async fn refresh_lounge_token(screen_id: &str) -> Result<Screen, LoungeError> {
        // Use shared client for better connection pooling
        let params = [("screen_ids", screen_id)];

        let response = SHARED_CLIENT
            .post("https://www.youtube.com/api/lounge/pairing/get_lounge_token_batch")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(LoungeError::InvalidResponse(format!(
                "Failed to refresh token: {}",
                response.status()
            )));
        }

        let screens_response = response.json::<ScreensResponse>().await?;

        if let Some(screen) = screens_response.screens.into_iter().next() {
            Ok(screen)
        } else {
            Err(LoungeError::InvalidResponse(
                "No screens returned".to_string(),
            ))
        }
    }

    // Check screen availability
    pub async fn check_screen_availability(&self) -> Result<bool, LoungeError> {
        let params = [("lounge_token", &self.lounge_token)]; // Use reference instead of clone

        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/pairing/get_screen_availability")
            .form(&params)
            .send()
            .await?;

        // Handle token expiration for this method too
        if response.status().as_u16() == 401 {
            return Err(LoungeError::TokenExpired);
        }

        if !response.status().is_success() {
            return Err(LoungeError::InvalidResponse(format!(
                "Failed to check screen availability: {}",
                response.status()
            )));
        }

        // Get the status before consuming the response
        let status = response.status().is_success();

        // Try to parse the response, but if it fails, just return the status
        match response.text().await {
            Ok(text) => {
                if let Ok(availability_response) =
                    serde_json::from_str::<ScreenAvailabilityResponse>(&text)
                {
                    if let Some(screen) = availability_response.screens.into_iter().next() {
                        Ok(screen.status == "online")
                    } else {
                        Ok(false) // No screens means not available
                    }
                } else {
                    // Couldn't parse the response, just return success status
                    Ok(status)
                }
            }
            Err(_) => {
                // Couldn't read the response, just return success status
                Ok(status)
            }
        }
    }

    /// Check screen availability with automatic token refresh
    pub async fn check_screen_availability_with_refresh(&mut self) -> Result<bool, LoungeError> {
        match self.check_screen_availability().await {
            Ok(available) => Ok(available),
            Err(LoungeError::TokenExpired) => {
                // Token expired, try to refresh it
                println!("Token expired, refreshing...");
                self.refresh_token_internal().await?;

                // Retry with new token
                println!("Retrying availability check with refreshed token");
                self.check_screen_availability().await
            }
            Err(e) => Err(e),
        }
    }

    // Connect to the screen and establish a session
    pub async fn connect(&self) -> Result<(), LoungeError> {
        // Reset session state before making any async calls
        {
            let session_state = self.session_state.lock();
            match session_state {
                Ok(mut state) => {
                    state.rid = 1;
                    state.command_offset = 0;
                    state.sid = None;
                    state.gsessionid = None;
                    state.aid = None;
                }
                Err(err) => {
                    return Err(LoungeError::MutexPoisoned(format!(
                        "Failed to reset session state: {:?}",
                        err
                    )))
                }
            }
        } // Lock is released here

        let params = [
            ("RID", "1"),
            ("VER", "8"),
            ("CVER", "1"),
            ("auth_failure_option", "send_error"),
        ];

        // Build the connect form data
        let form_data = self.build_connect_form_data();

        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(LoungeError::InvalidResponse(format!(
                "Failed to connect: {}",
                response.status()
            )));
        }

        let body = response.bytes().await?;

        // Extract session IDs from the response body
        let (sid, gsessionid) = extract_session_ids(&body)?;

        // Update session state with the IDs we obtained
        {
            match self.session_state.lock() {
                Ok(mut state) => {
                    state.sid = sid;
                    state.gsessionid = gsessionid;
                }
                Err(err) => {
                    return Err(LoungeError::MutexPoisoned(format!(
                        "Failed to update session IDs: {:?}",
                        err
                    )))
                }
            }
        } // Lock is released here

        // Set connected flag
        {
            match self.connected.lock() {
                Ok(mut connected) => *connected = true,
                Err(err) => {
                    return Err(LoungeError::MutexPoisoned(format!(
                        "Failed to update connected state: {:?}",
                        err
                    )))
                }
            }
        } // Lock is released here

        // Send session established event
        let handler = HttpResponseHandler::new(&self.connected, &self.event_sender);
        handler.send_event(LoungeEvent::SessionEstablished);

        // Start the event subscription loop
        self.subscribe_to_events().await?;

        Ok(())
    }

    // Subscribe to events from the screen
    async fn subscribe_to_events(&self) -> Result<(), LoungeError> {
        let session_state = self.session_state.clone();
        let device_name = self.device_name.clone();
        let lounge_token = self.lounge_token.clone();
        let event_sender = self.event_sender.clone();
        let connected = self.connected.clone();

        // Clone the session manager for use in the task
        let session_manager = self.session_manager.clone();

        task::spawn(async move {
            loop {
                // Break the loop if no longer connected or if mutex is poisoned
                let is_connected = match connected.lock() {
                    Ok(lock) => *lock,
                    Err(err) => {
                        eprintln!("Mutex poisoned in event loop: {:?}", err);
                        break; // Exit loop on mutex poisoning
                    }
                };

                if !is_connected {
                    break;
                }

                // Get session information and clone it before the await point
                let (sid, gsessionid, aid) = match session_state.lock() {
                    Ok(state) => {
                        // Check if the session is valid
                        if !state.is_valid() {
                            // Need to reconnect
                            drop(state); // explicitly release the lock
                            if let Err(err) = connected.lock().map(|mut c| *c = false) {
                                eprintln!("Failed to update connected state: {:?}", err);
                            }
                            break;
                        }

                        // Get the session info
                        state.get_session_info()
                    }
                    Err(err) => {
                        eprintln!("Failed to access session state in event loop: {:?}", err);
                        // Mark as disconnected
                        if let Err(lock_err) = connected.lock().map(|mut c| *c = false) {
                            eprintln!("Failed to update connected state: {:?}", lock_err);
                        }
                        break;
                    }
                };

                // Get the values from Options, but ensure they're valid
                let (sid_value, gsession_value) = match (sid, gsessionid) {
                    (Some(s), Some(g)) => (s, g),
                    _ => {
                        eprintln!("Missing session IDs in event loop");
                        if let Err(err) = connected.lock().map(|mut c| *c = false) {
                            eprintln!("Failed to update connected state: {:?}", err);
                        }
                        break;
                    }
                };

                let mut params = HashMap::new();
                params.insert("name", device_name.as_str());
                params.insert("loungeIdToken", lounge_token.as_str());
                params.insert("SID", sid_value.as_str());
                params.insert("gsessionid", gsession_value.as_str());
                params.insert("device", "REMOTE_CONTROL");
                params.insert("app", "youtube-desktop");
                params.insert("VER", "8");
                params.insert("v", "2");
                params.insert("RID", "rpc");
                params.insert("CI", "0");
                params.insert("TYPE", "xmlhttp");

                // Add AID if we have one
                if let Some(ref aid_value) = aid {
                    params.insert("AID", aid_value);
                }

                // Make the request using LONG_POLL_CLIENT for long polling connections
                let response = match LONG_POLL_CLIENT
                    .get("https://www.youtube.com/api/lounge/bc/bind")
                    .query(&params)
                    .send()
                    .await
                {
                    Ok(res) => res,
                    Err(e) => {
                        // Log the error and retry after a delay
                        eprintln!("Event subscription error: {}", e);
                        sleep(Duration::from_secs(RECONNECT_DELAY)).await;
                        continue;
                    }
                };

                // Create a response handler and check for errors
                let response_handler = HttpResponseHandler::new(&connected, &event_sender);

                // If response indicates an error, break or retry as appropriate
                if let Err(error) = response_handler.check_response(&response) {
                    match error {
                        // For terminal errors, break the loop
                        LoungeError::SessionExpired
                        | LoungeError::TokenExpired
                        | LoungeError::ConnectionClosed => break,

                        // For other errors, retry after a delay
                        _ => {
                            sleep(Duration::from_secs(RECONNECT_DELAY)).await;
                            continue;
                        }
                    }
                }

                // Process the streamed response
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();
                let mut size_buffer = String::new();
                let mut reading_size = true;
                let mut expected_size = 0;

                let mut processor = ChunkProcessor {
                    buffer: &mut buffer,
                    size_buffer: &mut size_buffer,
                    reading_size: &mut reading_size,
                    expected_size: &mut expected_size,
                    session_state: &session_state,
                    sender: &event_sender,
                    session_manager: &session_manager,
                };

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            // Process the bytes chunk
                            process_bytes_chunk(&chunk, &mut processor);
                        }
                        Err(e) => {
                            eprintln!("Error in stream: {}", e);
                            break;
                        }
                    }
                }

                // If we reach here, the connection was closed
                // Wait a moment before reconnecting
                sleep(Duration::from_secs(RECONNECT_SHORT_DELAY)).await;
            }

            // Function ended, client is disconnected
            if let Err(err) = connected.lock().map(|mut c| *c = false) {
                eprintln!(
                    "Failed to update connected state at end of event loop: {:?}",
                    err
                );
            }
        });

        Ok(())
    }

    /// Prepare common API request parameters
    fn prepare_api_params(&self, sid: &str, gsessionid: &str, rid: i32) -> Vec<(String, String)> {
        let rid_str = rid.to_string();

        vec![
            ("name".to_string(), self.device_name.clone()),
            ("loungeIdToken".to_string(), self.lounge_token.clone()),
            ("SID".to_string(), sid.to_string()),
            ("gsessionid".to_string(), gsessionid.to_string()),
            ("VER".to_string(), "8".to_string()),
            ("v".to_string(), "2".to_string()),
            ("RID".to_string(), rid_str),
        ]
    }

    /// Check connection state and session validity
    fn check_session_state(&self) -> Result<(String, String, i32, i32), LoungeError> {
        // Check if we're connected
        let is_connected = match self.connected.lock() {
            Ok(connected) => *connected,
            Err(err) => {
                return Err(LoungeError::MutexPoisoned(format!(
                    "Failed to check connected state: {:?}",
                    err
                )))
            }
        };

        if !is_connected {
            return Err(LoungeError::ConnectionClosed);
        }

        // Get the session state values we need
        let (sid, gsessionid, rid, ofs) = match self.session_state.lock() {
            Ok(mut state) => {
                // Check if the session is valid
                if !state.is_valid() {
                    return Err(LoungeError::SessionExpired);
                }

                // Get the values we need
                let (sid, gsessionid, _) = state.get_session_info();
                let rid = state.increment_rid();
                let ofs = state.increment_offset();

                (sid, gsessionid, rid, ofs)
            }
            Err(err) => {
                return Err(LoungeError::MutexPoisoned(format!(
                    "Failed to access session state: {:?}",
                    err
                )))
            }
        };

        // Convert to non-optional types now that we know they're valid
        match (sid, gsessionid) {
            (Some(s), Some(g)) => Ok((s, g, rid, ofs)),
            _ => Err(LoungeError::SessionExpired),
        }
    }

    /// Send a command to the screen
    pub async fn send_command(&self, command: PlaybackCommand) -> Result<(), LoungeError> {
        // Get and validate session state
        let (sid, gsessionid, rid, ofs) = self.check_session_state()?;
        let command_name = get_command_name(&command);

        // Prepare base parameters
        let mut params = self.prepare_api_params(&sid, &gsessionid, rid);
        params.push(("req0__sc".to_string(), command_name.to_string()));

        // Build the form data with the offset
        let mut form_data = format!("count=1&ofs={}", ofs);

        // Add command-specific parameters efficiently
        match &command {
            PlaybackCommand::SetPlaylist {
                video_id,
                current_index,
                list_id,
                current_time,
                audio_only,
                params,
                player_params,
            } => {
                // Required parameter
                form_data.push_str(&format!("&req0_videoId={}", video_id));

                // Optional parameters - only add if Some
                if let Some(idx) = current_index {
                    form_data.push_str(&format!("&req0_currentIndex={}", idx));
                } else {
                    // Default to -1 as per the Lounge API documentation
                    form_data.push_str("&req0_currentIndex=-1");
                }

                // Add list_id if provided
                if let Some(list) = list_id {
                    form_data.push_str(&format!("&req0_listId={}", list));
                } else {
                    // Empty listId as per documentation
                    form_data.push_str("&req0_listId=");
                }

                // Add current_time if provided
                if let Some(time) = current_time {
                    form_data.push_str(&format!("&req0_currentTime={}", time));
                } else {
                    // Default to 0 as per documentation
                    form_data.push_str("&req0_currentTime=0");
                }

                // Add audio_only if provided
                if let Some(audio) = audio_only {
                    form_data.push_str(&format!("&req0_audioOnly={}", audio));
                } else {
                    // Default to false as per documentation
                    form_data.push_str("&req0_audioOnly=false");
                }

                // Add params if provided
                if let Some(p) = params {
                    form_data.push_str(&format!("&req0_params={}", p));
                } else {
                    // Empty params as per documentation
                    form_data.push_str("&req0_params=");
                }

                // Add player_params if provided
                if let Some(pp) = player_params {
                    form_data.push_str(&format!("&req0_playerParams={}", pp));
                } else {
                    // Empty playerParams as per documentation
                    form_data.push_str("&req0_playerParams=");
                }

                // Add recommended param from documentation
                form_data.push_str("&req0_prioritizeMobileSenderPlaybackStateOnConnection=true");
            }
            PlaybackCommand::AddVideo {
                video_id,
                video_sources,
            } => {
                // Required parameter
                form_data.push_str(&format!("&req0_videoId={}", video_id));

                // Optional parameter
                if let Some(sources) = video_sources {
                    form_data.push_str(&format!("&req0_videoSources={}", sources));
                }
            }
            PlaybackCommand::SeekTo { new_time } => {
                form_data.push_str(&format!("&req0_newTime={}", new_time));
            }
            PlaybackCommand::SetAutoplayMode { autoplay_mode } => {
                form_data.push_str(&format!("&req0_autoplayMode={}", autoplay_mode));
            }
            PlaybackCommand::SetVolume { volume } => {
                form_data.push_str(&format!("&req0_volume={}", volume));
            }
            _ => {}
        }

        // Send the request
        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await?;

        // Check for errors using our handler
        let response_handler = HttpResponseHandler::new(&self.connected, &self.event_sender);
        response_handler.check_response(&response)?;

        Ok(())
    }

    /// Send a command with automatic token refresh if the token is expired
    pub async fn send_command_with_refresh(
        &mut self,
        command: PlaybackCommand,
    ) -> Result<(), LoungeError> {
        match self.send_command(command.clone()).await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                // Token expired, try to refresh it
                self.refresh_token_internal().await?;

                // Retry the command with the new token
                self.send_command(command).await
            }
            Err(e) => Err(e),
        }
    }

    /// Disconnect from the screen
    pub async fn disconnect(&self) -> Result<(), LoungeError> {
        // Only try to disconnect if connected
        let is_connected = match self.connected.lock() {
            Ok(connected) => *connected,
            Err(err) => {
                eprintln!(
                    "Failed to check connected state during disconnect: {:?}",
                    err
                );
                return Ok(()); // Assume not connected if mutex is poisoned
            }
        };

        if !is_connected {
            return Ok(());
        }

        // Get and validate session state (without offset)
        let (sid, gsessionid, rid, _) = match self.check_session_state() {
            Ok(state) => state,
            // If session is invalid, we're already effectively disconnected
            Err(_) => return Ok(()),
        };

        // Prepare common parameters
        let params = self.prepare_api_params(&sid, &gsessionid, rid);

        // Build the terminate form data
        let form_data = "ui=&TYPE=terminate&clientDisconnectReason=MDX_SESSION_DISCONNECT_REASON_DISCONNECTED_BY_USER";

        // Send the request - ignore errors since we'll mark as disconnected anyway
        let _ = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await;

        // Set connected to false
        if let Err(err) = self
            .connected
            .lock()
            .map(|mut connected| *connected = false)
        {
            eprintln!(
                "Failed to update connected state during disconnect: {:?}",
                err
            );
        }

        Ok(())
    }

    /// Build form data for initial connection request
    fn build_connect_form_data(&self) -> String {
        // Encode the device name first to avoid temporary value issues
        let encoded_name = urlencoding::encode(&self.device_name);

        // Use a Vec to build up the form parameters
        let params = vec![
            ("app", "web"),
            ("mdx-version", "3"),
            ("name", &encoded_name),
            // Per API docs, id should be empty for initial connection, not the screen_id
            ("id", ""),
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
            ("loungeIdToken", &self.lounge_token),
        ];

        // Join the parameters with '&'
        params
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<String>>()
            .join("&")
    }

    // Get video thumbnail URL
    pub fn get_thumbnail_url(video_id: &str, thumbnail_idx: u8) -> String {
        format!(
            "https://img.youtube.com/vi/{}/{}.jpg",
            video_id, thumbnail_idx
        )
    }
}

/// Helper function to extract session IDs from a response body
fn extract_session_ids(body: &Bytes) -> Result<(Option<String>, Option<String>), LoungeError> {
    let reader = BufReader::new(&body[..]);
    let mut sid = None;
    let mut gsessionid = None;

    // Parse the entire response at once to extract the session IDs
    let full_response = String::from_utf8_lossy(body).to_string();

    // Try to extract sid and gsessionid from the raw response
    let c_marker = "[\"c\",\"";
    if let Some(c_idx) = full_response.find(c_marker) {
        let sid_start = c_idx + c_marker.len();
        if let Some(sid_end) = full_response[sid_start..].find('\"') {
            sid = Some(full_response[sid_start..sid_start + sid_end].to_string());
        }
    }

    let s_marker = "[\"S\",\"";
    if let Some(s_idx) = full_response.find(s_marker) {
        let gsession_start = s_idx + s_marker.len();
        if let Some(gsession_end) = full_response[gsession_start..].find('\"') {
            gsessionid =
                Some(full_response[gsession_start..gsession_start + gsession_end].to_string());
        }
    }

    // If the above string approach fails, try to parse individual lines
    if sid.is_none() || gsessionid.is_none() {
        // Parse the chunked response line by line
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() || line.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }

            if sid.is_none() && line.contains("[\"c\",\"") {
                if let Some(start_idx) = line.find("[\"c\",\"") {
                    let start = start_idx + 5;
                    if let Some(end_idx) = line[start..].find('\"') {
                        sid = Some(line[start..start + end_idx].to_string());
                    }
                }
            }

            if gsessionid.is_none() && line.contains("[\"S\",\"") {
                if let Some(start_idx) = line.find("[\"S\",\"") {
                    let start = start_idx + 5;
                    if let Some(end_idx) = line[start..].find('\"') {
                        gsessionid = Some(line[start..start + end_idx].to_string());
                    }
                }
            }

            if sid.is_some() && gsessionid.is_some() {
                break;
            }
        }
    }

    // Check if we found the session IDs
    if sid.is_none() || gsessionid.is_none() {
        return Err(LoungeError::InvalidResponse(
            "Failed to obtain session IDs".to_string(),
        ));
    }

    Ok((sid, gsessionid))
}

// Helper struct to reduce number of arguments in process_bytes_chunk
struct ChunkProcessor<'a> {
    buffer: &'a mut String,
    size_buffer: &'a mut String,
    reading_size: &'a mut bool,
    expected_size: &'a mut usize,
    session_state: &'a Arc<Mutex<SessionState>>,
    sender: &'a broadcast::Sender<LoungeEvent>,
    session_manager: &'a PlaybackSessionManager,
}

/// A more efficient processor for YouTube API's chunked response format
fn process_bytes_chunk(chunk: &Bytes, processor: &mut ChunkProcessor) {
    // Only create UTF-8 string from portions we need to process
    let chunk_slice = chunk.as_ref();
    let mut i = 0;

    while i < chunk_slice.len() {
        if *processor.reading_size {
            // Find the newline if we're reading the size
            if let Some(newline_pos) = chunk_slice[i..].iter().position(|&b| b == b'\n') {
                // Add any digits to the size buffer
                let size_portion = &chunk_slice[i..i + newline_pos];
                if !size_portion.is_empty() {
                    // Only create a string for the size portion
                    let size_str = String::from_utf8_lossy(size_portion);
                    processor.size_buffer.push_str(&size_str);
                }

                // Parse the size and prepare for content
                *processor.expected_size = processor.size_buffer.parse::<usize>().unwrap_or(0);
                processor.size_buffer.clear();
                *processor.reading_size = false;

                // Move past the newline
                i += newline_pos + 1;
            } else {
                // Size continues in the next chunk
                let size_portion = &chunk_slice[i..];
                let size_str = String::from_utf8_lossy(size_portion);
                processor.size_buffer.push_str(&size_str);
                break;
            }
        } else {
            // Reading content - calculate how much more we need
            let remaining = *processor.expected_size - processor.buffer.len();
            let available = chunk_slice.len() - i;
            let to_read = remaining.min(available);

            if to_read > 0 {
                // Append directly to the buffer
                let content_slice = &chunk_slice[i..i + to_read];
                processor
                    .buffer
                    .push_str(&String::from_utf8_lossy(content_slice));
                i += to_read;
            }

            // Check if we've completed a message
            if processor.buffer.len() >= *processor.expected_size {
                // Process the complete chunk with the event pipeline
                let event_pipeline = EventPipeline::new(
                    processor.session_state,
                    processor.sender,
                    processor.session_manager,
                );
                event_pipeline.process_event_chunk(processor.buffer);

                processor.buffer.clear();
                *processor.reading_size = true;
            } else {
                // Need more data from the next chunk
                break;
            }
        }
    }
}

/// Unified event processing pipeline for YouTube Lounge API events
struct EventPipeline<'a> {
    session_state: &'a Arc<Mutex<SessionState>>,
    sender: &'a broadcast::Sender<LoungeEvent>,
    session_manager: &'a PlaybackSessionManager,
    debug_mode: bool,
}

impl<'a> EventPipeline<'a> {
    /// Create a new event pipeline
    fn new(
        session_state: &'a Arc<Mutex<SessionState>>,
        sender: &'a broadcast::Sender<LoungeEvent>,
        session_manager: &'a PlaybackSessionManager,
    ) -> Self {
        // Get debug mode setting from session state
        let debug_mode = match session_state.lock() {
            Ok(state) => state.debug_mode,
            Err(err) => {
                eprintln!("Failed to get debug mode setting: {:?}", err);
                false // Default to false if mutex is poisoned
            }
        };

        Self {
            session_state,
            sender,
            session_manager,
            debug_mode,
        }
    }

    /// Helper method to send events - logs send errors instead of discarding them
    #[inline]
    fn send_event(&self, event: LoungeEvent) {
        if let Err(err) = self.sender.send(event.clone()) {
            // Use an appropriate log level based on the event's importance
            eprintln!("Failed to broadcast event {}: {}", event.name(), err);
        }
    }

    /// Process an event chunk received from the YouTube API
    fn process_event_chunk(&self, chunk: &str) {
        if chunk.trim().is_empty() {
            return;
        }

        // Parse the JSON chunk
        let json_result = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(chunk);

        // Log the error and return early if JSON parsing fails
        let events = match json_result {
            Ok(data) => data,
            Err(err) => {
                eprintln!("Failed to parse event chunk: {} - Raw JSON: {}", err, chunk);
                return;
            }
        };

        // Process each event in the chunk
        for event in &events {
            self.process_single_event(event);
        }
    }

    /// Process a single event from the chunk
    fn process_single_event(&self, event: &[serde_json::Value]) {
        if event.len() < 2 {
            return;
        }

        // Extract and update the event ID (AID)
        if let Some(event_id) = event.first().and_then(|id| id.as_i64()) {
            // Update the session state with the event ID
            match self.session_state.lock() {
                Ok(mut state) => {
                    state.aid = Some(event_id.to_string());
                }
                Err(err) => {
                    eprintln!("Failed to update AID in session state: {:?}", err);
                }
            }
        }

        // Process the event data if it has the right structure
        if let Some(event_array) = event.get(1).and_then(|v| v.as_array()) {
            if event_array.len() < 2 {
                return;
            }

            if let Some(event_type) = event_array.first().and_then(|t| t.as_str()) {
                // Get a reference to the payload
                let payload = &event_array[1];
                // Handle the event with the appropriate handler
                self.handle_event(event_type, payload);
            }
        }
    }

    /// Process standard events with a simple parsing pattern and proper error logging
    fn process_simple_event<T, F>(&self, payload: &serde_json::Value, event_creator: F)
    where
        T: serde::de::DeserializeOwned,
        F: FnOnce(T) -> LoungeEvent,
        T: std::fmt::Debug, // Allow for debugging
    {
        match serde_json::from_value::<T>(payload.clone()) {
            Ok(data) => {
                self.send_event(event_creator(data));
            }
            Err(err) => {
                eprintln!(
                    "Failed to deserialize {} event: {} - Payload: {}",
                    std::any::type_name::<T>(),
                    err,
                    payload
                );
            }
        }
    }

    /// Handle the event with the appropriate handler based on event type
    fn handle_event(&self, event_type: &str, payload: &serde_json::Value) {
        debug_log!(
            self.debug_mode,
            "Event [{}] payload: {}",
            event_type,
            payload
        );

        match event_type {
            // Complex events with session state updates
            "onStateChange" => self.process_state_change(payload),
            "nowPlaying" => self.process_now_playing(payload),
            "loungeStatus" => self.process_lounge_status(payload),

            // Simple notification events
            "loungeScreenDisconnected" => self.send_event(LoungeEvent::ScreenDisconnected),

            // Simple events that follow the same pattern - grouped by category

            // Ad-related events
            "onAdStateChange" => {
                self.process_simple_event::<AdState, _>(payload, LoungeEvent::AdStateChange)
            }

            // Track selection events
            "onSubtitlesTrackChanged" => self.process_simple_event::<SubtitlesTrackChanged, _>(
                payload,
                LoungeEvent::SubtitlesTrackChanged,
            ),
            "onAudioTrackChanged" => self.process_simple_event::<AudioTrackChanged, _>(
                payload,
                LoungeEvent::AudioTrackChanged,
            ),

            // Playback control events
            "onAutoplayModeChanged" => self.process_simple_event::<AutoplayModeChanged, _>(
                payload,
                LoungeEvent::AutoplayModeChanged,
            ),
            "onHasPreviousNextChanged" => self.process_simple_event::<HasPreviousNextChanged, _>(
                payload,
                LoungeEvent::HasPreviousNextChanged,
            ),
            "onVideoQualityChanged" => self.process_simple_event::<VideoQualityChanged, _>(
                payload,
                LoungeEvent::VideoQualityChanged,
            ),
            "onVolumeChanged" => {
                self.process_simple_event::<VolumeChanged, _>(payload, LoungeEvent::VolumeChanged)
            }

            // Playlist-related events
            "playlistModified" => self.process_simple_event::<PlaylistModified, _>(
                payload,
                LoungeEvent::PlaylistModified,
            ),
            "autoplayUpNext" => {
                self.process_simple_event::<AutoplayUpNext, _>(payload, LoungeEvent::AutoplayUpNext)
            }

            // Unknown events
            _ => {
                let event_with_payload = format!("{} - payload: {}", event_type, payload);
                debug_log!(
                    self.debug_mode,
                    "Unknown event [{}] payload: {}",
                    event_type,
                    payload
                );
                self.send_event(LoungeEvent::Unknown(event_with_payload));
            }
        }
    }

    /// Process a state change event with proper error handling
    fn process_state_change(&self, payload: &serde_json::Value) {
        match serde_json::from_value::<PlaybackState>(payload.clone()) {
            Ok(state) => {
                // Use the session manager to process state change
                if state.cpn.is_some() {
                    // Update existing session or create a new one
                    self.session_manager.process_state_change(&state);
                }

                self.send_event(LoungeEvent::StateChange(state));
            }
            Err(err) => {
                eprintln!(
                    "Failed to deserialize StateChange event: {} - Payload: {}",
                    err, payload
                );
            }
        }
    }

    /// Process a now playing event with proper error handling
    fn process_now_playing(&self, payload: &serde_json::Value) {
        match serde_json::from_value::<NowPlaying>(payload.clone()) {
            Ok(mut now_playing) => {
                // Ensure video_data fields are populated
                now_playing.video_data = VideoData {
                    video_id: now_playing.video_id.clone(),
                    author: "".to_string(),
                    title: "".to_string(),
                    is_playable: true,
                };

                // Use the session manager to process the now playing event
                if now_playing.cpn.is_some() {
                    self.session_manager.process_now_playing(&now_playing);
                }

                self.send_event(LoungeEvent::NowPlaying(now_playing));
            }
            Err(err) => {
                eprintln!(
                    "Failed to deserialize NowPlaying event: {} - Payload: {}",
                    err, payload
                );
            }
        }
    }

    /// Process a lounge status event with proper error handling for all deserialization steps
    fn process_lounge_status(&self, payload: &serde_json::Value) {
        // First, try to parse the main LoungeStatus
        match serde_json::from_value::<LoungeStatus>(payload.clone()) {
            Ok(status) => {
                // Now try to parse the nested devices JSON
                match serde_json::from_str::<Vec<Device>>(&status.devices) {
                    Ok(devices) => {
                        // Process device info with error handling
                        let devices_with_info: Vec<Device> = devices
                            .into_iter()
                            .map(|mut device| {
                                match serde_json::from_str::<DeviceInfo>(&device.device_info_raw) {
                                    Ok(info) => {
                                        device.device_info = Some(info);
                                    },
                                    Err(err) => {
                                        eprintln!(
                                            "Failed to deserialize DeviceInfo for device {}: {} - Raw: '{}'", 
                                            device.id,
                                            err,
                                            device.device_info_raw
                                        );
                                        // Still proceed with the device, even without device_info
                                    }
                                }
                                device
                            })
                            .collect();

                        // Use the session manager to process device list
                        self.session_manager
                            .process_device_list(&devices_with_info, status.queue_id.as_ref());

                        self.send_event(LoungeEvent::LoungeStatus(
                            devices_with_info,
                            status.queue_id.clone(),
                        ));
                    }
                    Err(err) => {
                        eprintln!(
                            "Failed to deserialize devices list: {} - Raw devices: {}",
                            err, status.devices
                        );

                        // Still fire the event with an empty device list to maintain connection
                        self.session_manager
                            .process_device_list(&[], status.queue_id.as_ref());
                        self.send_event(LoungeEvent::LoungeStatus(
                            vec![], // Empty device list
                            status.queue_id.clone(),
                        ));
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "Failed to deserialize LoungeStatus event: {} - Payload: {}",
                    err, payload
                );
            }
        }
    }
}
