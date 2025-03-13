use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::debug_log;
use crate::models::{Device, NowPlaying, PlaybackSession, PlaybackState};
use crate::utils::state::{HasPlaybackState, PlaybackStatus};

/// Manages a single playback session for a device/screen
#[derive(Clone)]
pub struct PlaybackSessionManager {
    // The current active session
    current_session: Arc<Mutex<Option<PlaybackSession>>>,

    // The device ID for this session manager's screen
    device_id: Arc<Mutex<String>>,

    // Channel for broadcasting session updates
    session_sender: broadcast::Sender<PlaybackSession>,

    // Debug mode setting
    debug_mode: Arc<Mutex<bool>>,
}

impl PlaybackSessionManager {
    /// Create a new session manager for a specific device
    pub fn new(device_id: &str) -> Self {
        // Create a broadcast channel for playback sessions
        let (session_tx, _) = broadcast::channel(10);

        Self {
            current_session: Arc::new(Mutex::new(None)),
            device_id: Arc::new(Mutex::new(device_id.to_string())),
            session_sender: session_tx,
            debug_mode: Arc::new(Mutex::new(false)),
        }
    }

    /// Helper method to send session updates - discards send errors
    #[inline]
    fn send_session(&self, session: PlaybackSession) {
        let _ = self.session_sender.send(session);
    }

    /// Get a channel for receiving session updates
    pub fn subscribe(&self) -> broadcast::Receiver<PlaybackSession> {
        self.session_sender.subscribe()
    }

    /// Enable debug mode for verbose logging
    pub fn enable_debug_mode(&self) {
        let mut debug = self.debug_mode.lock().unwrap();
        *debug = true;
    }

    /// Disable debug mode
    pub fn disable_debug_mode(&self) {
        let mut debug = self.debug_mode.lock().unwrap();
        *debug = false;
    }

    /// Check if debug mode is enabled
    pub fn is_debug_mode(&self) -> bool {
        *self.debug_mode.lock().unwrap()
    }

    /// Update session from a NowPlaying event
    pub fn process_now_playing(&self, event: &NowPlaying) -> Option<PlaybackSession> {
        // Create a new session from the event
        if let Some(mut session) = PlaybackSession::from_now_playing(event) {
            // Set the device ID from this manager
            let device_id = self.device_id.lock().unwrap().clone();
            session.device_id = Some(device_id);

            // Update the current session
            let mut current = self.current_session.lock().unwrap();
            *current = Some(session.clone());

            // Broadcast the session update
            self.send_session(session.clone());

            return Some(session);
        }
        None
    }

    /// Update session from a state change event
    pub fn process_state_change(&self, event: &PlaybackState) -> Option<PlaybackSession> {
        let mut updated_session = None;

        // Lock current session
        let mut current = self.current_session.lock().unwrap();

        if let Some(session) = current.as_mut() {
            // Update existing session if we have one
            if session.update_from_state_change(event) {
                updated_session = Some(session.clone());
            }
        } else if let Some(mut new_session) = PlaybackSession::from_state_change(event) {
            // Create a new session if we don't have one
            let device_id = self.device_id.lock().unwrap().clone();
            new_session.device_id = Some(device_id);

            *current = Some(new_session.clone());
            updated_session = Some(new_session);
        }

        // Release lock
        drop(current);

        // Send update if session was modified or created
        if let Some(ref session) = updated_session {
            self.send_session(session.clone());
        }

        updated_session
    }

    /// Process device info from lounge status
    pub fn process_device_info(&self, device: &Device) {
        let mut current = self.current_session.lock().unwrap();
        if let Some(session) = current.as_mut() {
            // Update device details in session if needed
            if session.device_id != Some(device.id.clone()) {
                session.device_id = Some(device.id.clone());

                // Notify of update
                let updated = session.clone();
                drop(current);
                self.send_session(updated);
            }
        }
    }

    /// Process a device list and update device information
    pub fn process_device_list(&self, devices: &[Device], queue_id: Option<&String>) {
        if let Some(queue_id) = queue_id {
            debug_log!(
                self.is_debug_mode(),
                "Processing device list with queue_id {} and {} devices",
                queue_id,
                devices.len()
            );

            // Find our REMOTE_CONTROL device
            let remote_device = devices.iter().find(|d| d.device_type == "REMOTE_CONTROL");

            if let Some(remote_device) = remote_device {
                let device_id = remote_device.id.clone();
                debug_log!(
                    self.is_debug_mode(),
                    "Found remote device with ID: {}",
                    device_id
                );
                self.process_device_info(remote_device);
            }
        }
    }

    /// Get the current session if it exists
    pub fn get_current_session(&self) -> Option<PlaybackSession> {
        let current = self.current_session.lock().unwrap();
        current.clone()
    }

    /// Get the current session if it exists and matches a specific CPN
    pub fn get_session_by_cpn(&self, cpn: &str) -> Option<PlaybackSession> {
        let current = self.current_session.lock().unwrap();
        if let Some(session) = current.as_ref() {
            if session.cpn == cpn {
                return Some(session.clone());
            }
        }
        None
    }

    /// Check if the current session has a specific status
    pub fn has_session_with_status(&self, status: PlaybackStatus) -> bool {
        let current = self.current_session.lock().unwrap();
        if let Some(session) = current.as_ref() {
            return session.status() == status;
        }
        false
    }

    /// Check if the current session is playing
    pub fn has_playing_session(&self) -> bool {
        self.has_session_with_status(PlaybackStatus::Playing)
    }

    /// Check if the current session is for a specific video
    pub fn has_session_with_video_id(&self, video_id: &str) -> bool {
        let current = self.current_session.lock().unwrap();
        if let Some(session) = current.as_ref() {
            return session.video_id.as_deref() == Some(video_id);
        }
        false
    }

    /// Clear the current session (e.g., on disconnect)
    pub fn clear_session(&self) {
        let mut current = self.current_session.lock().unwrap();
        *current = None;
    }
}

impl Default for PlaybackSessionManager {
    fn default() -> Self {
        Self::new("default")
    }
}
