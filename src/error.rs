use thiserror::Error;

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

    #[error("Session not established or lost (Missing SID/GSID)")]
    SessionLost,

    #[error("Token expired (HTTP 401)")]
    TokenExpired, // Indicates a 401 was received

    #[error("Connection explicitly closed or terminated")]
    ConnectionClosed, // E.g. disconnect() called, or maybe 410 Gone

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Server indicated session is invalid (HTTP {0})")]
    SessionInvalidatedByServer(u16),

    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(Box<LoungeError>), // Box to avoid recursive type size issue

    #[error("Task panicked or cancelled")]
    TaskJoinError(#[from] tokio::task::JoinError),
}

impl LoungeError {
    /// Helper to check if an error suggests the session is definitively dead
    /// (requires a full re-bind attempt).
    pub(crate) fn _indicates_session_dead(&self) -> bool {
        matches!(
            self,
            LoungeError::SessionLost
                | LoungeError::SessionInvalidatedByServer(_)
                | LoungeError::ConnectionClosed // Treat explicit close/410 as dead
        )
    }
}
