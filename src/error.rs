use std::error::Error;
use std::fmt;
use std::io;
use std::sync::PoisonError;

// Custom error type for the YouTube Lounge API
#[derive(Debug)]
pub enum LoungeError {
    RequestError(reqwest::Error),
    ParseError {
        error: serde_json::Error,
        context: String,
        payload: String,
    },
    IoError(io::Error),
    InvalidResponse(String),
    SessionExpired,
    TokenExpired,
    ConnectionClosed,
    BroadcastError(String),
    MutexPoisoned(String),
    Other(String),
}

impl fmt::Display for LoungeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoungeError::RequestError(e) => write!(f, "Request error: {}", e),
            LoungeError::ParseError {
                error,
                context,
                payload,
            } => {
                write!(
                    f,
                    "Parse error: {} in {} with payload: {}",
                    error, context, payload
                )
            }
            LoungeError::IoError(e) => write!(f, "IO error: {}", e),
            LoungeError::InvalidResponse(s) => write!(f, "Invalid response: {}", s),
            LoungeError::SessionExpired => write!(f, "Session expired"),
            LoungeError::TokenExpired => write!(f, "Lounge token expired"),
            LoungeError::ConnectionClosed => write!(f, "Connection closed"),
            LoungeError::BroadcastError(s) => write!(f, "Broadcast send error: {}", s),
            LoungeError::MutexPoisoned(s) => write!(f, "Mutex poisoned: {}", s),
            LoungeError::Other(s) => write!(f, "Other error: {}", s),
        }
    }
}

impl Error for LoungeError {}

impl From<io::Error> for LoungeError {
    fn from(err: io::Error) -> Self {
        LoungeError::IoError(err)
    }
}

impl From<reqwest::Error> for LoungeError {
    fn from(err: reqwest::Error) -> Self {
        LoungeError::RequestError(err)
    }
}

impl From<serde_json::Error> for LoungeError {
    fn from(err: serde_json::Error) -> Self {
        LoungeError::ParseError {
            error: err,
            context: "No context provided".to_string(),
            payload: "Unknown payload".to_string(),
        }
    }
}

// Generic implementation for mutex poisoning errors
impl<T> From<PoisonError<T>> for LoungeError {
    fn from(err: PoisonError<T>) -> Self {
        LoungeError::MutexPoisoned(format!("Mutex poisoned: {:?}", err))
    }
}

// Helper functions for creating specific error types

/// Create a parse error with full context and payload information
pub fn parse_error(
    error: serde_json::Error,
    context: impl Into<String>,
    payload: impl Into<String>,
) -> LoungeError {
    LoungeError::ParseError {
        error,
        context: context.into(),
        payload: payload.into(),
    }
}

/// Handle broadcast send errors with context about what was being sent
pub fn broadcast_error<T: fmt::Display>(err: T, event_type: impl Into<String>) -> LoungeError {
    LoungeError::BroadcastError(format!(
        "Failed to broadcast {} event: {}",
        event_type.into(),
        err
    ))
}

/// Log an error with the given message
pub fn log_error(err: &LoungeError, message: &str) {
    eprintln!("{}: {}", message, err);
}
