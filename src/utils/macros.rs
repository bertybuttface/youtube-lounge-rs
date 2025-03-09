// Utility macros for use throughout the codebase

/// Macro for debug logging - only logs when debug_mode is true
///
/// # Examples
///
/// ```ignore
/// // Import the macro
/// use youtube_lounge_rs::debug_log;
///
/// // Example usage with a debug flag
/// let debug_mode = true;
/// debug_log!(debug_mode, "Processing event: {}", "test_event");
/// ```
#[macro_export]
macro_rules! debug_log {
    ($debug_mode:expr, $($arg:tt)*) => {
        if $debug_mode {
            println!("DEBUG: {}", format!($($arg)*));
        }
    };
}
