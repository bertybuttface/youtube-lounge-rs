// Playback commands
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Next,
    Previous,
    SkipAd,
    /// Set and play a video, replacing current playlist
    ///
    /// This command starts playback of a new video, replacing the current playlist.
    SetPlaylist {
        video_id: String,
        // Optional parameters with defaults
        #[doc(hidden)]
        current_index: Option<i32>,
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

    /// Create AddVideo command from just a video_id
    pub fn add_video(video_id: String) -> Self {
        PlaybackCommand::AddVideo {
            video_id,
            video_sources: None,
        }
    }
}
