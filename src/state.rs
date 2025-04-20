use std::sync::{atomic::AtomicU32, Arc};

use crate::TokenCallback;

/// Represents the observable state of the background connection manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state or after explicit disconnection.
    Disconnected,
    /// Attempting the initial bind or re-bind after disconnection/error.
    Connecting,
    /// Successfully bound and actively polling for events.
    Connected,
    /// A recoverable error occurred, waiting before retrying connection.
    WaitingToReconnect { backoff: std::time::Duration },
    /// An unrecoverable error occurred (e.g., invalid screen ID, repeated auth failures).
    Failed(String), // Include an error message
    /// The manager task is shutting down (e.g., after disconnect() or Drop).
    Stopping,
}

// Represents the outcome of a connection manager cycle (poll or bind attempt)
#[derive(Debug)]
pub enum ConnectionStatus {
    Success,            // Operation succeeded (data processed, stream ended, bind successful)
    SessionInvalidated, // Server indicated session is dead (400, 404, 410)
    TokenExpired,       // Server indicated token is expired (401)
}

// Shared state containing token and refresh callback
pub(crate) struct InnerState {
    pub(crate) lounge_token: String,
    pub(crate) token_refresh_callback: TokenCallback,
}

// Shared state representing the current session status
// Wrapped in Arc<RwLock<>> in LoungeClient
#[derive(Clone, Debug)] // Added Debug
pub(crate) struct SessionState {
    pub(crate) sid: Option<String>,
    pub(crate) gsessionid: Option<String>,
    // Atomics are already thread-safe, Arc makes them shareable
    pub(crate) rid: Arc<AtomicU32>,
    pub(crate) command_offset: Arc<AtomicU32>,
}

impl SessionState {
    pub(crate) fn new() -> Self {
        Self {
            sid: None,
            gsessionid: None,
            // Start RID at 1, subsequent increments happen before use
            rid: Arc::new(AtomicU32::new(1)),
            // Start offset at 0, subsequent increments happen before use
            command_offset: Arc::new(AtomicU32::new(0)),
        }
    }
}
