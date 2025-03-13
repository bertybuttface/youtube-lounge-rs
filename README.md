# YouTube Lounge API Client

A Rust client library for the YouTube Lounge API, which allows controlling YouTube playback on TV devices and other connected displays.

[![Crates.io](https://img.shields.io/crates/v/youtube-lounge-rs.svg)](https://crates.io/crates/youtube-lounge-rs)
[![Docs.rs](https://docs.rs/youtube-lounge-rs/badge.svg)](https://docs.rs/youtube-lounge-rs)
[![CI](https://github.com/bertybuttface/youtube-lounge-rs/workflows/CI/badge.svg)](https://github.com/bertybuttface/youtube-lounge-rs/actions/workflows/ci.yml)
[![License: CC BY-NC 4.0](https://img.shields.io/badge/License-CC%20BY--NC%204.0-lightgrey.svg)](https://creativecommons.org/licenses/by-nc/4.0/)

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
  - [Quick Start](#quick-start)
  - [Pairing with a Screen](#pairing-with-a-screen)
  - [Creating a Client](#creating-a-client)
  - [Connecting to a Screen](#connecting-to-a-screen)
  - [Receiving Events](#receiving-events)
  - [Controlling Playback](#controlling-playback)
  - [Disconnecting](#disconnecting)
- [Examples](#examples)
- [API Reference](#api-reference)
  - [PlaybackSession Management](#playbacksession-management)
  - [Debug Mode](#debug-mode)
  - [PlaybackCommand](#playbackcommand)
  - [LoungeEvent](#loungeevent)
- [Release Process](#release-process)
- [License](#license)

## Features

### Core Features
- Pair with YouTube-enabled TVs and devices using pairing codes
- Control playback (play, pause, volume, seek, etc.)
- Receive real-time playback status updates
- Queue and manage videos for playback

### Advanced Features
- Handle reconnection and token refresh logic
- Track playback session state with detailed information
- Monitor playback state changes in real-time
- Manage YouTube playlists

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
youtube-lounge-rs = "3.0.1"
```

### Dependencies

This library requires:
- Rust 1.56 or later
- `tokio` for async runtime
- `reqwest` for HTTP requests
- Internet connectivity for YouTube API access

## Usage

### Quick Start

```rust
use youtube_lounge_rs::{LoungeClient, PlaybackCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Pair with a screen (TV) using a pairing code displayed on the device
    let screen = LoungeClient::pair_with_screen("ABC123").await?;
    
    // 2. Create a client to control the screen
    let mut client = LoungeClient::new(
        &screen.screen_id,
        &screen.lounge_token,
        "My Rust Remote"
    );
    
    // 3. Set up event handling
    let mut rx = client.event_receiver();
    let mut session_rx = client.session_receiver();
    
    // 4. Connect to the screen
    client.connect().await?;
    
    // 5. Send commands to control playback
    client.send_command(PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string())).await?;
    client.send_command(PlaybackCommand::Pause).await?;
    client.send_command(PlaybackCommand::Play).await?;
    
    // 6. Disconnect when done
    client.disconnect().await?;
    
    Ok(())
}
```

### Pairing with a screen

```rust
let screen = LoungeClient::pair_with_screen("ABC123").await?;
println!("Paired with: {}", screen.name.unwrap_or_default());
```

### Creating a client

```rust
let client = LoungeClient::new(
    &screen.screen_id,
    &screen.lounge_token,
    "My Rust Remote"
);
```

### Connecting to a screen

```rust
// Check if screen is available
if client.check_screen_availability().await? {
    // Connect to the screen
    client.connect().await?;
}
```

### Receiving events

```rust
let mut rx = client.event_receiver();

// Process events in a loop
while let Some(event) = rx.recv().await {
    match event {
        LoungeEvent::StateChange(state) => {
            println!("Playback state: {}", state.state);
        },
        LoungeEvent::NowPlaying(now_playing) => {
            println!("Now playing: {}", now_playing.video_data.title);
        },
        LoungeEvent::ScreenDisconnected => {
            println!("Screen disconnected");
            break;
        },
        LoungeEvent::AdStateChange(ad_state) => {
            println!("Ad playing. Content video: {}", ad_state.content_video_id);
            println!("Skip enabled: {}", ad_state.is_skip_enabled);
        },
        LoungeEvent::SubtitlesTrackChanged(track) => {
            println!("Subtitles track changed for video: {}", track.video_id);
        },
        LoungeEvent::AutoplayModeChanged(mode) => {
            println!("Autoplay mode changed to: {}", mode.autoplay_mode);
        },
        LoungeEvent::HasPreviousNextChanged(nav) => {
            println!("Navigation changed - Next: {}, Previous: {}", 
                nav.has_next, nav.has_previous);
        },
        LoungeEvent::VideoQualityChanged(quality) => {
            println!("Video quality changed to {} for {}", 
                quality.quality_level, quality.video_id);
        },
        LoungeEvent::AudioTrackChanged(audio) => {
            println!("Audio track changed to {} for {}", 
                audio.audio_track_id, audio.video_id);
        },
        LoungeEvent::PlaylistModified(playlist) => {
            println!("Playlist modified: Video {} in list {}", 
                playlist.video_id, playlist.list_id);
        },
        LoungeEvent::AutoplayUpNext(next) => {
            println!("Autoplay up next: {}", next.video_id);
        },
        // Handle other events...
        _ => {}
    }
}
```

### Controlling playback

```rust
// Play a specific video (recommended method)
client.send_command(PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string())).await?;

// Play a YouTube playlist by ID
client.send_command(PlaybackCommand::set_playlist_by_id("PLxxxx".to_string())).await?;

// Play a specific video in a playlist by index
client.send_command(PlaybackCommand::set_playlist_with_index("PLxxxx".to_string(), 3)).await?;

// Add a video to the queue (will play after current video)
client.send_command(PlaybackCommand::add_video("QH2-TGUlwu4".to_string())).await?;

// Manual construction (advanced usage with all parameters)
client.send_command(PlaybackCommand::SetPlaylist { 
    video_id: "dQw4w9WgXcQ".to_string(),
    current_index: Some(-1),
    list_id: None,
    current_time: Some(0.0),
    audio_only: Some(false),
    params: None,
    player_params: None,
}).await?;

// Pause playback
client.send_command(PlaybackCommand::Pause).await?;

// Resume playback
client.send_command(PlaybackCommand::Play).await?;

// Seek to a specific position (in seconds)
client.send_command(PlaybackCommand::SeekTo { 
    new_time: 30.0 
}).await?;

// Adjust volume (0-100)
client.send_command(PlaybackCommand::SetVolume { 
    volume: 50 
}).await?;

// Skip to the next video in a playlist
client.send_command(PlaybackCommand::Next).await?;
```

### Disconnecting

```rust
client.disconnect().await?;
```

## Examples

The library includes two example applications to help you understand its usage.

### Basic Usage Example

A simple example that demonstrates the core functionality:

```bash
cargo run --example basic_usage <your_pairing_code>
```

This example demonstrates:
- Pairing with a screen
- Connecting to the device
- Creating event and session receivers
- Sending commands (play, pause, seek, volume)
- Receiving and handling events

### Advanced Example with Persistence

A more comprehensive example with session persistence:

```bash
# First time: pair with a screen
cargo run --example advanced_usage pair <your_pairing_code>

# Subsequent runs: reuse stored authentication
cargo run --example advanced_usage

# Run with debug mode to see raw event JSON
cargo run --example advanced_usage debug

# Pair and enable debug mode
cargo run --example advanced_usage pair <your_pairing_code> debug

# Show help for all commands
cargo run --example advanced_usage help
```

Advanced example features:
- **Persistent Authentication**: Stores screen information in a JSON file
- **Multi-Screen Control**: Connects to and controls multiple paired screens simultaneously
- **Command-Line Interface**: Supports different modes via command-line arguments
- **Token Refresh**: Automatically handles token refreshing and persistence
- **Debug Mode**: Option to display raw JSON for all events
- **Parallel Commands**: Sends commands to multiple screens in parallel
- **Session Tracking**: Demonstrates all session query methods
- **Comprehensive Event Handling**: Shows handling for all event types

## Release Process

This library follows semantic versioning and uses GitHub Actions for automated releases:

1. **Version Updates**: When updating the version in `Cargo.toml`, follow semver:
   - `0.1.0` → `0.2.0` for non-breaking feature additions
   - `0.1.0` → `0.1.1` for bug fixes
   - `0.1.0` → `1.0.0` for major or breaking changes

2. **Creating a Release**:
   ```bash
   # Update version in Cargo.toml
   # Commit the changes
   git tag v0.1.0
   git push origin v0.1.0
   ```

3. **Automated Workflow**: When a tag is pushed, the GitHub Actions workflow:
   - Runs tests, linting and code coverage
   - Publishes to crates.io
   - Creates a GitHub release with auto-generated changelog

## API Reference

The library provides the following main components:

### `LoungeClient`

The main client for interacting with the YouTube Lounge API.

#### PlaybackSession Management

The library provides a `PlaybackSession` tracking system that maintains the state of videos being played on a connected device. Unlike regular events which are transient, sessions persist and provide a consolidated view of the current playback state:

```rust
// Get a receiver for session updates
let mut session_rx = client.session_receiver();

// Process session updates in a separate task
tokio::spawn(async move {
    while let Ok(session) = session_rx.recv().await {
        println!("Device ID: {}", session.device_id.as_deref().unwrap_or("Unknown"));
        println!("Video: {}", session.video_id.as_deref().unwrap_or("Unknown"));
        println!("Progress: {:.2}/{:.2} ({:.1}%)", 
            session.current_time, 
            session.duration,
            session.progress_percentage());
        println!("State: {}", session.state_name());
        
        // Sessions track playback history when available
        if let Some(history) = &session.video_history {
            println!("Video history: {} videos", history.len());
        }
    }
});

// Get the current session (most recent)
if let Some(current) = client.get_current_session() {
    println!("Current session: {}", current.video_id.unwrap_or_default());
    println!("State: {}", current.state_name());
    println!("Progress: {:.1}%", current.progress_percentage());
}

// Check session state with convenience methods
if client.has_playing_session() {
    println!("There is a video currently playing");
}

if client.has_session_with_video_id("VIDEO_ID") {
    println!("Found session for the specific video");
}

// Find a session by its CPN (Client Playback Nonce)
if let Some(session) = client.get_session_by_cpn("some-cpn-value") {
    println!("Found session for video: {}", session.video_id.unwrap_or_default());
}
```

Sessions provide a more reliable way to track playback state and maintain continuity between events, especially useful for applications that need to maintain playback state and history information. The session is updated automatically as events arrive from the device.

#### Debug Mode

You can enable debug mode to see the raw JSON payload of all events, which helps when inspecting for new or undocumented parameters:

```rust
// Enable debug mode to inspect all events and their raw JSON
client.enable_debug_mode();

// Later, when done debugging
client.disable_debug_mode();
```

When debug mode is enabled, all events (including unknown ones) will print their full JSON payload to the console, allowing you to see any parameters that aren't currently captured in the model structures.

### `PlaybackCommand`

Commands that can be sent to control playback:

#### Basic Control Commands
- `Play` - Resume playback
- `Pause` - Pause playback
- `Next` - Skip to next video
- `Previous` - Go to previous video
- `SkipAd` - Skip current advertisement
- `SeekTo { new_time: f64 }` - Seek to specific position
- `SetAutoplayMode { autoplay_mode: String }` - Change autoplay settings
- `SetVolume { volume: i32 }` - Set volume level (0-100)
- `Mute` - Mute audio
- `Unmute` - Unmute audio

#### Content Commands

**Play a single video:**
```rust
// Recommended approach
PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string())

// Full manual construction
PlaybackCommand::SetPlaylist { 
    video_id: "dQw4w9WgXcQ".to_string(),
    current_index: Some(-1),
    list_id: None,
    current_time: Some(0.0),
    audio_only: Some(false),
    params: None,
    player_params: None,
}
```

**Play a YouTube playlist:**
```rust
// Play from beginning of playlist
PlaybackCommand::set_playlist_by_id("PLxxxx".to_string())

// Play specific video in playlist by index
PlaybackCommand::set_playlist_with_index("PLxxxx".to_string(), 3)
```

**Add a video to queue:**
```rust
// Add to end of current queue
PlaybackCommand::add_video("QH2-TGUlwu4".to_string())
```

### `LoungeEvent`

Events received from the YouTube Lounge API:

- `StateChange(PlaybackState)`
- `NowPlaying(NowPlaying)`
- `LoungeStatus(Vec<Device>)`
- `ScreenDisconnected`
- `SessionEstablished`
- `AdStateChange(AdState)`
- `SubtitlesTrackChanged(SubtitlesTrackChanged)`
- `AutoplayModeChanged(AutoplayModeChanged)`
- `HasPreviousNextChanged(HasPreviousNextChanged)`
- `VideoQualityChanged(VideoQualityChanged)`
- `AudioTrackChanged(AudioTrackChanged)`
- `PlaylistModified(PlaylistModified)`
- `AutoplayUpNext(AutoplayUpNext)`
- `Unknown(String)`

## License

This project is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License (CC BY-NC 4.0) - see the LICENSE file for details.

Important: This license prohibits any commercial use of this code without explicit permission from the copyright holder.