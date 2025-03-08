/// Trait for parsing YouTube API string values into appropriate types
pub trait YoutubeValueParser {
    /// Parse a numeric string into a float, defaulting to 0.0 if parsing fails
    fn parse_float(s: &str) -> f64 {
        s.parse::<f64>().unwrap_or(0.0)
    }

    /// Parse a numeric string into an integer, defaulting to 0 if parsing fails
    fn parse_int(s: &str) -> i32 {
        s.parse::<i32>().unwrap_or(0)
    }

    /// Parse a boolean string, treating "true" (case-sensitive) as true
    fn parse_bool(s: &str) -> bool {
        s == "true"
    }

    /// Parse a comma-separated list into a vector of strings
    fn parse_list(s: &str) -> Vec<String> {
        s.split(',').map(|s| s.trim().to_string()).collect()
    }
}

/// Implement the parser for common types
impl YoutubeValueParser for str {}

/// Trait for types that have YouTube playback duration fields
pub trait HasDuration {
    /// Get the current playback time in seconds
    fn current_time(&self) -> f64;

    /// Get the total duration in seconds
    fn duration(&self) -> f64;

    /// Get the loaded time in seconds (buffered content)
    fn loaded_time(&self) -> f64;

    /// Get the seekable start time
    fn seekable_start_time(&self) -> f64;

    /// Get the seekable end time
    fn seekable_end_time(&self) -> f64;

    /// Calculate the progress percentage (0-100)
    fn progress_percentage(&self) -> f64 {
        if self.duration() > 0.0 {
            (self.current_time() / self.duration()) * 100.0
        } else {
            0.0
        }
    }
}

/// Trait for types that have volume information
pub trait HasVolume {
    /// Get the volume level (0-100)
    fn volume(&self) -> i32;

    /// Check if audio is muted
    fn is_muted(&self) -> bool;
}
