use std::error::Error;
use std::fmt;
use std::io;

// Custom error type for the YouTube Lounge API
#[derive(Debug)]
pub enum LoungeError {
    RequestError(reqwest::Error),
    ParseError(serde_json::Error),
    IoError(io::Error),
    InvalidResponse(String),
    SessionExpired,
    TokenExpired,
    ConnectionClosed,
    Other(String),
}

impl fmt::Display for LoungeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoungeError::RequestError(e) => write!(f, "Request error: {}", e),
            LoungeError::ParseError(e) => write!(f, "Parse error: {}", e),
            LoungeError::IoError(e) => write!(f, "IO error: {}", e),
            LoungeError::InvalidResponse(s) => write!(f, "Invalid response: {}", s),
            LoungeError::SessionExpired => write!(f, "Session expired"),
            LoungeError::TokenExpired => write!(f, "Lounge token expired"),
            LoungeError::ConnectionClosed => write!(f, "Connection closed"),
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
        LoungeError::ParseError(err)
    }
}
