use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::debug_log;
use crate::models::{Device, NowPlaying, PlaybackSession, PlaybackState};
use crate::utils::state::PlaybackStatus;

/// Manages playback sessions and their relationship to devices
#[derive(Clone)]
pub struct PlaybackSessionManager {
    // Sessions indexed by Content Playback Network ID (CPN)
    active_sessions: Arc<Mutex<HashMap<String, PlaybackSession>>>,

    // Mapping from playlist ID to device ID
    list_id_to_device: Arc<Mutex<HashMap<String, String>>>,

    // Channel for broadcasting session updates
    session_sender: broadcast::Sender<PlaybackSession>,

    // Debug mode setting
    debug_mode: Arc<Mutex<bool>>,
}

impl PlaybackSessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        // Create a broadcast channel for playback sessions with capacity for 100 sessions
        let (session_tx, _) = broadcast::channel(100);

        Self {
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            list_id_to_device: Arc::new(Mutex::new(HashMap::new())),
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

    /// Get or create a session from a NowPlaying event
    pub fn process_now_playing(&self, event: &NowPlaying) -> Option<PlaybackSession> {
        let cpn = event.cpn.clone()?;

        // Create a new session from the event
        if let Some(session) = PlaybackSession::from_now_playing(event) {
            let mut sessions = self.active_sessions.lock().unwrap();

            // Store or update the session
            let session_clone = session.clone();
            sessions.insert(cpn.clone(), session);
            drop(sessions);

            // Update list_id to device mapping if available
            self.update_list_id_mapping(event);

            // Broadcast the session update
            self.send_session(session_clone.clone());

            // Return the created session
            return Some(session_clone);
        }

        None
    }

    /// Update an existing session from a state change event
    pub fn process_state_change(&self, event: &PlaybackState) -> Option<PlaybackSession> {
        let cpn = event.cpn.clone()?;

        let mut updated_session = None;

        // First, update any existing session
        {
            let mut sessions = self.active_sessions.lock().unwrap();

            if let Some(session) = sessions.get_mut(&cpn) {
                // Update existing session
                if session.update_from_state_change(event) {
                    updated_session = Some(session.clone());
                }
            } else if let Some(new_session) = PlaybackSession::from_state_change(event) {
                // Create a new session
                updated_session = Some(new_session.clone());
                sessions.insert(cpn.clone(), new_session);
            }
        }

        // Send update if session was modified or created
        if let Some(ref session) = updated_session {
            self.send_session(session.clone());
        }

        updated_session
    }

    /// Process a device list and map device IDs to sessions
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
                    "Mapping device_id {} to queue_id {}",
                    device_id,
                    queue_id
                );

                // Update the 1:1 mapping
                {
                    let mut list_map = self.list_id_to_device.lock().unwrap();
                    list_map.insert(queue_id.clone(), device_id.clone());

                    debug_log!(
                        self.is_debug_mode(),
                        "Added mapping: list_id {} -> device_id {}",
                        queue_id,
                        device_id
                    );
                }

                // Update device_id field for any sessions with matching list_id
                {
                    let mut sessions = self.active_sessions.lock().unwrap();
                    let mut updated_sessions = Vec::new();

                    for session in sessions.values_mut() {
                        if session.list_id.as_deref() == Some(queue_id) {
                            debug_log!(
                                self.is_debug_mode(),
                                "Updating session for CPN {} with device_id {}",
                                session.cpn,
                                device_id
                            );

                            if session.device_id != Some(device_id.clone()) {
                                session.device_id = Some(device_id.clone());
                                updated_sessions.push(session.clone());
                            }
                        }
                    }

                    // Release lock before sending updates
                    drop(sessions);

                    // Send updates for any modified sessions
                    for session in updated_sessions {
                        self.send_session(session);
                    }
                }
            }
        }
    }

    // Helper function to update list_id to device mapping from NowPlaying event
    fn update_list_id_mapping(&self, event: &NowPlaying) {
        if let Some(list_id) = &event.list_id {
            if let Some(cpn) = &event.cpn {
                // First check if we have a device_id for this list_id
                let device_id = {
                    let list_map = self.list_id_to_device.lock().unwrap();
                    list_map.get(list_id).cloned()
                };

                // If we have a device_id, update the session
                if let Some(device_id) = device_id {
                    let mut sessions = self.active_sessions.lock().unwrap();
                    if let Some(session) = sessions.get_mut(cpn) {
                        debug_log!(
                            self.is_debug_mode(),
                            "Updating session for CPN {} with device_id {} from list_id mapping",
                            cpn,
                            device_id
                        );
                        session.device_id = Some(device_id);
                    }
                }
            }
        }
    }

    /// Get session by CPN (Content Playback Network ID)
    pub fn get_session_by_cpn(&self, cpn: &str) -> Option<PlaybackSession> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.get(cpn).cloned()
    }

    /// Get session by device ID through list_id mapping
    pub fn get_session_for_device(&self, device_id: &str) -> Option<PlaybackSession> {
        // Find list_id associated with this device (reverse lookup)
        let list_id_to_device = self.list_id_to_device.lock().unwrap();
        let found_list_id = list_id_to_device.iter().find_map(|(list_id, dev_id)| {
            if dev_id == device_id {
                Some(list_id.clone())
            } else {
                None
            }
        });

        drop(list_id_to_device);

        if let Some(list_id) = found_list_id {
            // Find session with this list_id
            let sessions = self.active_sessions.lock().unwrap();
            sessions
                .values()
                .find(|s| s.list_id.as_deref() == Some(&list_id))
                .cloned()
        } else {
            None
        }
    }

    /// Get most recent session
    pub fn get_current_session(&self) -> Option<PlaybackSession> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.values().max_by_key(|s| s.last_updated).cloned()
    }

    /// Get all active sessions
    pub fn get_all_sessions(&self) -> Vec<PlaybackSession> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.values().cloned().collect()
    }

    /// Get sessions by playback status
    pub fn get_sessions_by_status(&self, status: PlaybackStatus) -> Vec<PlaybackSession> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions
            .values()
            .filter(|s| s.status() == status)
            .cloned()
            .collect()
    }

    /// Get sessions by video ID
    pub fn get_sessions_by_video_id(&self, video_id: &str) -> Vec<PlaybackSession> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions
            .values()
            .filter(|s| s.video_id.as_deref() == Some(video_id))
            .cloned()
            .collect()
    }

    /// Get currently playing sessions
    pub fn get_playing_sessions(&self) -> Vec<PlaybackSession> {
        self.get_sessions_by_status(PlaybackStatus::Playing)
    }

    /// Clear all sessions (e.g., on disconnect)
    pub fn clear_sessions(&self) {
        let mut sessions = self.active_sessions.lock().unwrap();
        sessions.clear();
    }
}

impl Default for PlaybackSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
