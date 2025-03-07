use bytes::Bytes;
use futures::StreamExt;
use once_cell::sync::Lazy;
use reqwest::{Client, ClientBuilder};
use serde_json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task;
use tokio::time::sleep;

// Create a static shared HTTP client for better connection pooling and DNS caching
static SHARED_CLIENT: Lazy<Client> = Lazy::new(|| {
    ClientBuilder::new()
        .timeout(Duration::from_secs(300)) // 5 minute timeout for regular requests
        .pool_idle_timeout(Some(Duration::from_secs(90))) // Keep connections alive longer
        .build()
        .unwrap()
});

// Create a separate client for long polling with a timeout slightly longer than the heartbeat interval
// The YouTube API should send at least a NOOP message every 30 minutes
static LONG_POLL_CLIENT: Lazy<Client> = Lazy::new(|| {
    ClientBuilder::new()
        .timeout(Duration::from_secs(32 * 60)) // 32 minutes timeout
        .pool_idle_timeout(Some(Duration::from_secs(90))) // Keep connections alive longer
        .build()
        .unwrap()
});

use crate::commands::{get_command_name, PlaybackCommand};
use crate::error::LoungeError;
use crate::events::LoungeEvent;
use crate::models::{
    AdState, AutoplayModeChanged, Device, DeviceInfo, LoungeStatus, NowPlaying, PlaybackState,
    Screen, ScreenAvailabilityResponse, ScreenResponse, ScreensResponse, SubtitlesTrackChanged,
};

// Session state
#[derive(Debug, Clone)]
struct SessionState {
    sid: Option<String>,
    gsessionid: Option<String>,
    aid: Option<String>,
    rid: i32,
    command_offset: i32,
}

impl SessionState {
    fn new() -> Self {
        SessionState {
            sid: None,
            gsessionid: None,
            aid: None,
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
}

impl LoungeClient {
    pub fn new(screen_id: &str, lounge_token: &str, device_name: &str) -> Self {
        // Use the shared client to benefit from connection pooling and DNS caching
        let client = SHARED_CLIENT.clone();

        // Create a broadcast channel with capacity for 100 events
        let (tx, _rx) = broadcast::channel(100);

        LoungeClient {
            client,
            screen_id: screen_id.to_string(),
            lounge_token: lounge_token.to_string(),
            device_name: device_name.to_string(),
            session_state: Arc::new(Mutex::new(SessionState::new())),
            event_sender: tx,
            connected: Arc::new(Mutex::new(false)),
            token_refresh_callback: None,
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

    /// Internal method to refresh the token
    async fn refresh_token_internal(&mut self) -> Result<(), LoungeError> {
        // Call the static refresh method
        let screen = Self::refresh_lounge_token(&self.screen_id).await?;

        // Update the client's token
        self.lounge_token = screen.lounge_token.clone();

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
        let mut session_state = self.session_state.lock().unwrap();
        session_state.rid = 1;
        session_state.command_offset = 0;
        session_state.sid = None;
        session_state.gsessionid = None;
        session_state.aid = None;
        drop(session_state);

        let params = [
            ("RID", "1"),
            ("VER", "8"),
            ("CVER", "1"),
            ("auth_failure_option", "send_error"),
        ];

        let form_data = format!(
            "app=web&mdx-version=3&name={}&id={}&device=REMOTE_CONTROL&capabilities=que,dsdtr,atp&method=setPlaylist&magnaKey=cloudPairedDevice&ui=false&deviceContext=user_agent=dunno&window_width_points=&window_height_points=&os_name=android&ms=&theme=cl&loungeIdToken={}",
            urlencoding::encode(&self.device_name),
            self.screen_id,
            self.lounge_token
        );

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
        let reader = BufReader::new(&body[..]);

        let mut sid = None;
        let mut gsessionid = None;

        // Parse the entire response at once to extract the session IDs
        let full_response = String::from_utf8_lossy(&body).to_string();

        // Try to extract sid and gsessionid from the raw response
        let c_marker = "[\"c\",\"";
        if let Some(c_idx) = full_response.find(c_marker) {
            let sid_start = c_idx + c_marker.len();
            if let Some(sid_end) = full_response[sid_start..].find("\"") {
                sid = Some(full_response[sid_start..sid_start + sid_end].to_string());
            }
        }

        let s_marker = "[\"S\",\"";
        if let Some(s_idx) = full_response.find(s_marker) {
            let gsession_start = s_idx + s_marker.len();
            if let Some(gsession_end) = full_response[gsession_start..].find("\"") {
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
                        if let Some(end_idx) = line[start..].find("\"") {
                            sid = Some(line[start..start + end_idx].to_string());
                        }
                    }
                }

                if gsessionid.is_none() && line.contains("[\"S\",\"") {
                    if let Some(start_idx) = line.find("[\"S\",\"") {
                        let start = start_idx + 5;
                        if let Some(end_idx) = line[start..].find("\"") {
                            gsessionid = Some(line[start..start + end_idx].to_string());
                        }
                    }
                }

                if sid.is_some() && gsessionid.is_some() {
                    break;
                }
            }
        }

        let mut state = self.session_state.lock().unwrap();
        state.sid = sid.clone();
        state.gsessionid = gsessionid.clone();
        drop(state);

        if sid.is_none() || gsessionid.is_none() {
            return Err(LoungeError::InvalidResponse(
                "Failed to obtain session IDs".to_string(),
            ));
        }

        // Set connected flag
        *self.connected.lock().unwrap() = true;

        // Send session established event
        let _ = self.event_sender.send(LoungeEvent::SessionEstablished);

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

        task::spawn(async move {
            loop {
                // Break the loop if no longer connected
                let is_connected = {
                    let lock = connected.lock().unwrap();
                    *lock
                };

                if !is_connected {
                    break;
                }

                // Get session information and clone it before the await point
                let (sid, gsessionid, aid) = {
                    let state = session_state.lock().unwrap();
                    (
                        state.sid.clone(),
                        state.gsessionid.clone(),
                        state.aid.clone(),
                    )
                };

                if sid.is_none() || gsessionid.is_none() {
                    // Need to reconnect
                    *connected.lock().unwrap() = false;
                    break;
                }

                // Avoid unnecessary cloning by using static strings where possible
                // and reusing the unwrapped values
                let sid_value = sid.unwrap();
                let gsession_value = gsessionid.unwrap();

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
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                // Check for specific error status codes
                match response.status().as_u16() {
                    400 => {
                        // Session expired
                        *connected.lock().unwrap() = false;
                        let _ = event_sender.send(LoungeEvent::ScreenDisconnected);
                        break;
                    }
                    401 => {
                        // Token expired
                        *connected.lock().unwrap() = false;
                        let _ = event_sender.send(LoungeEvent::ScreenDisconnected);
                        break;
                    }
                    410 => {
                        // Session gone
                        *connected.lock().unwrap() = false;
                        let _ = event_sender.send(LoungeEvent::ScreenDisconnected);
                        break;
                    }
                    _ => {}
                }

                if !response.status().is_success() {
                    // Other error, retry after a delay
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }

                // Process the streamed response
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();
                let mut size_buffer = String::new();
                let mut reading_size = true;
                let mut expected_size = 0;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            // Process the bytes chunk
                            process_bytes_chunk(
                                &chunk,
                                &mut buffer,
                                &mut size_buffer,
                                &mut reading_size,
                                &mut expected_size,
                                &session_state,
                                &event_sender,
                            )
                            .await;
                        }
                        Err(e) => {
                            eprintln!("Error in stream: {}", e);
                            break;
                        }
                    }
                }

                // If we reach here, the connection was closed
                // Wait a moment before reconnecting
                sleep(Duration::from_secs(1)).await;
            }

            // Function ended, client is disconnected
            *connected.lock().unwrap() = false;
        });

        Ok(())
    }

    // Send a command to the screen
    pub async fn send_command(&self, command: PlaybackCommand) -> Result<(), LoungeError> {
        // Check if we're connected
        if !*self.connected.lock().unwrap() {
            return Err(LoungeError::ConnectionClosed);
        }

        let mut state = self.session_state.lock().unwrap();
        let sid = state.sid.clone();
        let gsessionid = state.gsessionid.clone();
        let rid = state.increment_rid();
        let ofs = state.increment_offset();
        drop(state);

        if sid.is_none() || gsessionid.is_none() {
            return Err(LoungeError::SessionExpired);
        }

        // Build the params - getting unwrapped values
        let sid_value = sid.as_ref().unwrap();
        let gsession_value = gsessionid.as_ref().unwrap();
        let rid_str = rid.to_string();
        let command_name = get_command_name(&command);

        // Using references where possible to avoid unnecessary clones
        // Making sure all values are of the same type (&str)
        let params = [
            ("name", self.device_name.as_str()),
            ("loungeIdToken", self.lounge_token.as_str()),
            ("SID", sid_value.as_str()),
            ("gsessionid", gsession_value.as_str()),
            ("VER", "8"),
            ("v", "2"),
            ("RID", rid_str.as_str()),
            ("req0__sc", command_name.as_str()),
        ];

        // Build the form data
        let mut form_data = format!("count=1&ofs={}", ofs);

        // Add command-specific parameters
        match &command {
            PlaybackCommand::SetPlaylist { video_id } => {
                form_data.push_str(&format!("&req0_videoId={}", video_id));
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

        // Handle errors
        match response.status().as_u16() {
            400 => {
                *self.connected.lock().unwrap() = false;
                return Err(LoungeError::SessionExpired);
            }
            401 => {
                *self.connected.lock().unwrap() = false;
                return Err(LoungeError::TokenExpired);
            }
            410 => {
                *self.connected.lock().unwrap() = false;
                return Err(LoungeError::ConnectionClosed);
            }
            _ => {}
        }

        if !response.status().is_success() {
            return Err(LoungeError::InvalidResponse(format!(
                "Command failed: {}",
                response.status()
            )));
        }

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

    // Disconnect from the screen
    pub async fn disconnect(&self) -> Result<(), LoungeError> {
        // Only try to disconnect if connected
        if !*self.connected.lock().unwrap() {
            return Ok(());
        }

        let mut state = self.session_state.lock().unwrap();
        let sid = state.sid.clone();
        let gsessionid = state.gsessionid.clone();
        let rid = state.increment_rid();
        drop(state);

        if sid.is_none() || gsessionid.is_none() {
            return Ok(());
        }

        // Build the params
        let params = [
            ("name", self.device_name.clone()),
            ("loungeIdToken", self.lounge_token.clone()),
            ("SID", sid.unwrap()),
            ("gsessionid", gsessionid.unwrap()),
            ("VER", "8".to_string()),
            ("v", "2".to_string()),
            ("RID", rid.to_string()),
        ];

        // Build the form data
        let form_data = "ui=&TYPE=terminate&clientDisconnectReason=MDX_SESSION_DISCONNECT_REASON_DISCONNECTED_BY_USER";

        // Send the request
        let _ = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await;

        // Set connected to false
        *self.connected.lock().unwrap() = false;

        Ok(())
    }

    // Get video thumbnail URL
    pub fn get_thumbnail_url(video_id: &str, thumbnail_idx: u8) -> String {
        format!(
            "https://img.youtube.com/vi/{}/{}.jpg",
            video_id, thumbnail_idx
        )
    }
}

// Process a chunk of bytes from the stream more efficiently
async fn process_bytes_chunk(
    chunk: &Bytes,
    buffer: &mut String,
    size_buffer: &mut String,
    reading_size: &mut bool,
    expected_size: &mut usize,
    session_state: &Arc<Mutex<SessionState>>,
    sender: &broadcast::Sender<LoungeEvent>,
) {
    // Only create UTF-8 string from portions we need to process
    let chunk_slice = chunk.as_ref();
    let mut i = 0;

    while i < chunk_slice.len() {
        if *reading_size {
            // Find the newline if we're reading the size
            if let Some(newline_pos) = chunk_slice[i..].iter().position(|&b| b == b'\n') {
                // Add any digits to the size buffer
                let size_portion = &chunk_slice[i..i + newline_pos];
                if !size_portion.is_empty() {
                    // Only create a string for the size portion
                    let size_str = String::from_utf8_lossy(size_portion);
                    size_buffer.push_str(&size_str);
                }

                // Parse the size and prepare for content
                *expected_size = size_buffer.parse::<usize>().unwrap_or(0);
                size_buffer.clear();
                *reading_size = false;

                // Move past the newline
                i += newline_pos + 1;
            } else {
                // Size continues in the next chunk
                let size_portion = &chunk_slice[i..];
                let size_str = String::from_utf8_lossy(size_portion);
                size_buffer.push_str(&size_str);
                break;
            }
        } else {
            // Reading content - calculate how much more we need
            let remaining = *expected_size - buffer.len();
            let available = chunk_slice.len() - i;
            let to_read = remaining.min(available);

            if to_read > 0 {
                // Append directly to the buffer
                let content_slice = &chunk_slice[i..i + to_read];
                buffer.push_str(&String::from_utf8_lossy(content_slice));
                i += to_read;
            }

            // Check if we've completed a message
            if buffer.len() >= *expected_size {
                // Process the complete chunk
                process_event_chunk(buffer, session_state, sender).await;

                buffer.clear();
                *reading_size = true;
            } else {
                // Need more data from the next chunk
                break;
            }
        }
    }
}

// Helper function to process event chunks more efficiently
async fn process_event_chunk(
    chunk: &str,
    session_state: &Arc<Mutex<SessionState>>,
    sender: &broadcast::Sender<LoungeEvent>,
) {
    if chunk.trim().is_empty() {
        return;
    }

    // Using a reference to avoid cloning where possible
    let json_result = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(chunk);

    // Return early if JSON parsing fails
    let data = match json_result {
        Ok(data) => data,
        Err(_) => return,
    };

    for event in &data {
        if event.len() < 2 {
            continue;
        }

        // Extract the event ID and update the AID
        if let Some(event_id) = event.first().and_then(|id| id.as_i64()) {
            // Update the session state with the event ID
            let mut state = session_state.lock().unwrap();
            state.aid = Some(event_id.to_string());
            drop(state);
        }

        // Process the event data if it has the right structure
        if let Some(event_array) = event.get(1).and_then(|v| v.as_array()) {
            if event_array.len() < 2 {
                continue;
            }

            if let Some(event_type) = event_array.first().and_then(|t| t.as_str()) {
                // Get a reference to the payload
                let payload = &event_array[1];

                match event_type {
                    "onStateChange" => {
                        // Convert JSON value to PlaybackState directly
                        if let Ok(state) = serde_json::from_value::<PlaybackState>(payload.clone())
                        {
                            let _ = sender.send(LoungeEvent::StateChange(state));
                        }
                    }
                    "nowPlaying" => {
                        if let Ok(now_playing) =
                            serde_json::from_value::<NowPlaying>(payload.clone())
                        {
                            let _ = sender.send(LoungeEvent::NowPlaying(now_playing));
                        }
                    }
                    "loungeStatus" => {
                        if let Ok(status) = serde_json::from_value::<LoungeStatus>(payload.clone())
                        {
                            // Parse nested JSON - try to avoid unnecessary string conversions
                            let devices_result =
                                serde_json::from_str::<Vec<Device>>(&status.devices);

                            if let Ok(devices) = devices_result {
                                // Process device info efficiently
                                let devices_with_info: Vec<Device> = devices
                                    .into_iter()
                                    .map(|mut device| {
                                        if let Ok(info) = serde_json::from_str::<DeviceInfo>(
                                            &device.device_info_raw,
                                        ) {
                                            device.device_info = Some(info);
                                        }
                                        device
                                    })
                                    .collect();

                                let _ = sender.send(LoungeEvent::LoungeStatus(devices_with_info));
                            }
                        }
                    }
                    "loungeScreenDisconnected" => {
                        let _ = sender.send(LoungeEvent::ScreenDisconnected);
                    }
                    "onAdStateChange" => {
                        if let Ok(ad_state) = serde_json::from_value::<AdState>(payload.clone()) {
                            let _ = sender.send(LoungeEvent::AdStateChange(ad_state));
                        }
                    }
                    "onSubtitlesTrackChanged" => {
                        if let Ok(track) =
                            serde_json::from_value::<SubtitlesTrackChanged>(payload.clone())
                        {
                            let _ = sender.send(LoungeEvent::SubtitlesTrackChanged(track));
                        }
                    }
                    "onAutoplayModeChanged" => {
                        if let Ok(mode) =
                            serde_json::from_value::<AutoplayModeChanged>(payload.clone())
                        {
                            let _ = sender.send(LoungeEvent::AutoplayModeChanged(mode));
                        }
                    }
                    _ => {
                        // Unknown event - include payload for debugging
                        let event_with_payload = format!("{} - payload: {}", event_type, payload);
                        let _ = sender.send(LoungeEvent::Unknown(event_with_payload));
                    }
                }
            }
        }
    }
}
