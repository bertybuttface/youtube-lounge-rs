// Playback commands
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Next,
    Previous,
    SkipAd,
    SetPlaylist { video_id: String },
    SeekTo { new_time: f64 },
    SetAutoplayMode { autoplay_mode: String },
    SetVolume { volume: i32 },
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
        PlaybackCommand::SeekTo { .. } => "seekTo".to_string(),
        PlaybackCommand::SetAutoplayMode { .. } => "setAutoplayMode".to_string(),
        PlaybackCommand::SetVolume { .. } => "setVolume".to_string(),
    }
}
