use once_cell::sync::Lazy;
use std::{env, time::Duration};

/// Holds all tunables, read-once from ENV with fallbacks.
pub struct Settings {
    pub streaming_buffer_capacity: usize,
    pub event_buffer_capacity: usize,
    pub inactivity_timeout: Duration,
    pub min_backoff: Duration,
    pub max_backoff: Duration,
    pub request_timeout: Duration,
    pub long_poll_timeout: Duration,
}

impl Settings {
    fn from_env() -> Self {
        // optionally load .env
        let _ = dotenv::dotenv();

        // helper to parse usize
        fn parse_usize(var: &str, default: usize) -> usize {
            env::var(var)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }

        // helper to parse seconds into Duration
        fn parse_secs(var: &str, default_secs: u64) -> Duration {
            env::var(var)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or_else(|| Duration::from_secs(default_secs))
        }

        // helper to parse millis into Duration
        fn parse_millis(var: &str, default_ms: u64) -> Duration {
            env::var(var)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_millis)
                .unwrap_or_else(|| Duration::from_millis(default_ms))
        }

        Settings {
            streaming_buffer_capacity: parse_usize("STREAMING_BUFFER_CAPACITY", 16 * 1024),
            event_buffer_capacity: parse_usize("EVENT_BUFFER_CAPACITY", 1_000),
            inactivity_timeout: parse_secs("INACTIVITY_TIMEOUT_SECS", 32),
            min_backoff: parse_millis("MIN_BACKOFF_MS", 500),
            max_backoff: parse_secs("MAX_BACKOFF_SECS", 60),
            request_timeout: parse_secs("REQUEST_TIMEOUT_SECS", 10),
            long_poll_timeout: parse_secs("LONG_POLL_TIMEOUT_SECS", 300),
        }
    }
}

/// Global settings instance
pub static SETTINGS: Lazy<Settings> = Lazy::new(Settings::from_env);
