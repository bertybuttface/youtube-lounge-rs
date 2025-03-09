// Playback commands
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Next,
    Previous,
    SkipAd,
    /// Set and play a video or playlist, replacing current content
    ///
    /// This command starts playback of a new video or playlist, replacing the current content.
    /// At least one of video_id or list_id must have a value.
    SetPlaylist {
        /// ID of the video to play (can be empty if list_id is provided)
        video_id: String,
        // Optional parameters with defaults
        #[doc(hidden)]
        current_index: Option<i32>,
        /// ID of the playlist to play (optional, but required if video_id is empty)
        #[doc(hidden)]
        list_id: Option<String>,
        #[doc(hidden)]
        current_time: Option<f64>,
        #[doc(hidden)]
        audio_only: Option<bool>,
        #[doc(hidden)]
        params: Option<String>,
        #[doc(hidden)]
        player_params: Option<String>,
    },
    /// Add a video to the queue without interrupting current playback
    ///
    /// This command adds a video to the end of the current playlist
    /// without interrupting the currently playing video.
    AddVideo {
        video_id: String,
        // Optional parameter
        #[doc(hidden)]
        video_sources: Option<String>,
    },
    SeekTo {
        new_time: f64,
    },
    SetAutoplayMode {
        autoplay_mode: String,
    },
    SetVolume {
        volume: i32,
    },
    Mute,
    Unmute,
}

// Helper function to get the command name for a PlaybackCommand
pub fn get_command_name(command: &PlaybackCommand) -> String {
    match command {
        PlaybackCommand::Play => "play".to_string(),
        PlaybackCommand::Pause => "pause".to_string(),
        PlaybackCommand::Next => "next".to_string(),
        PlaybackCommand::Previous => "previous".to_string(),
        PlaybackCommand::SkipAd => "skipAd".to_string(),
        PlaybackCommand::SetPlaylist { .. } => "setPlaylist".to_string(),
        PlaybackCommand::AddVideo { .. } => "addVideo".to_string(),
        PlaybackCommand::SeekTo { .. } => "seekTo".to_string(),
        PlaybackCommand::SetAutoplayMode { .. } => "setAutoplayMode".to_string(),
        PlaybackCommand::SetVolume { .. } => "setVolume".to_string(),
        PlaybackCommand::Mute => "mute".to_string(),
        PlaybackCommand::Unmute => "unMute".to_string(),
    }
}

// Implementation block for direct command construction
impl PlaybackCommand {
    /// Create SetPlaylist command from just a video_id
    ///
    /// This plays a specific video by its ID, replacing any current content.
    /// All optional parameters will have sensible defaults.
    pub fn set_playlist(video_id: String) -> Self {
        PlaybackCommand::SetPlaylist {
            video_id,
            current_index: Some(-1),
            list_id: None,
            current_time: Some(0.0),
            audio_only: Some(false),
            params: None,
            player_params: None,
        }
    }

    /// Create SetPlaylist command to play a playlist by its ID
    ///
    /// This plays a specific playlist, starting from the first video.
    /// All optional parameters will have sensible defaults.
    pub fn set_playlist_by_id(list_id: String) -> Self {
        PlaybackCommand::SetPlaylist {
            video_id: "".to_string(), // Empty when playing by playlist ID
            current_index: Some(0),   // Start from the beginning of playlist
            list_id: Some(list_id),
            current_time: Some(0.0),
            audio_only: Some(false),
            params: None,
            player_params: None,
        }
    }

    /// Create SetPlaylist command to play a specific video in a playlist
    ///
    /// This plays a specific video by its index in a playlist.
    /// All optional parameters will have sensible defaults.
    pub fn set_playlist_with_index(list_id: String, index: i32) -> Self {
        PlaybackCommand::SetPlaylist {
            video_id: "".to_string(), // Empty when playing by playlist ID
            current_index: Some(index),
            list_id: Some(list_id),
            current_time: Some(0.0),
            audio_only: Some(false),
            params: None,
            player_params: None,
        }
    }

    /// Create AddVideo command from just a video_id
    ///
    /// This adds a video to the end of the current queue without interrupting playback.
    pub fn add_video(video_id: String) -> Self {
        PlaybackCommand::AddVideo {
            video_id,
            video_sources: None,
        }
    }
}
