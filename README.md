# YouTube Lounge API Client

A lightweight Rust client library for the YouTube Lounge API, which allows controlling YouTube playback on TV devices and other connected displays.

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
- [Release Process](#release-process)
- [License](#license)

## Features

- Pair with YouTube-enabled TVs and devices using pairing codes
- Control playback (play, pause, volume, seek, etc.)
- Receive real-time playback status updates
- Queue and manage videos for playback
- Debug mode for inspecting raw event data
- Automatic token refresh for persistent sessions
- Lightweight and simple API with minimal dependencies

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
youtube-lounge-rs = "8.0.4"
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
    let mut event_rx = client.event_receiver();
    
    // 4. Connect to the screen
    client.connect().await?;
    
    // 5. Send commands to control playback
    client.send_command_with_refresh(
        PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string())
    ).await?;
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
// Basic client with auto-generated device ID
let client = LoungeClient::new(
    &screen.screen_id,
    &screen.lounge_token,
    "My Rust Remote"
);

// Client with persistent device ID
let client = LoungeClient::with_device_id(
    &screen.screen_id,
    &screen.lounge_token,
    "My Rust Remote",
    "custom-device-id-123"
);
```

### Connecting to a screen

```rust
// Check if screen is available
if client.check_screen_availability().await? {
    // Connect to the screen
    client.connect().await?;
}

// With automatic token refresh
if client.check_screen_availability_with_refresh().await? {
    client.connect().await?;
}
```

### Receiving events

```rust
let mut rx = client.event_receiver();

// Process events in a loop
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            LoungeEvent::StateChange(state) => {
                println!("Playback state: {}", state.state);
                println!("Current time: {}s", state.current_time);
                println!("Duration: {}s", state.duration);
            },
            LoungeEvent::NowPlaying(now_playing) => {
                println!("Now playing video: {}", now_playing.video_id);
            },
            LoungeEvent::PlaybackSession(session) => {
                // This is a synthetic event that combines NowPlaying and StateChange
                println!("Video: {}, Position: {}s / {}s", 
                    session.video_id, session.current_time, session.duration);
                println!("State: {}, List ID: {:?}", session.state, session.list_id);
            },
            LoungeEvent::ScreenDisconnected => {
                println!("Screen disconnected");
                break;
            },
            LoungeEvent::SessionEstablished => {
                println!("Session established");
            },
            LoungeEvent::AdStateChange(ad_state) => {
                println!("Ad playing. Content video: {}", ad_state.content_video_id);
                println!("Skip enabled: {}", ad_state.is_skippable());
            },
            LoungeEvent::SubtitlesTrackChanged(track) => {
                println!("Subtitles track changed for video: {}", track.video_id);
            },
            LoungeEvent::AutoplayModeChanged(mode) => {
                println!("Autoplay mode changed to: {}", mode.autoplay_mode);
            },
            LoungeEvent::HasPreviousNextChanged(nav) => {
                let has_next = <str as YoutubeValueParser>::parse_bool(&nav.has_next);
                let has_prev = <str as YoutubeValueParser>::parse_bool(&nav.has_previous);
                println!("Navigation changed - Next: {}, Previous: {}", has_next, has_prev);
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
            LoungeEvent::Unknown(event_info) => {
                println!("Unknown event: {}", event_info);
            },
            _ => {}
        }
    }
});
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

// Use automatic token refresh with any command
client.send_command_with_refresh(PlaybackCommand::Play).await?;
```

### Disconnecting

```rust
client.disconnect().await?;
```

## YouTube Event Behavior

### NowPlaying Events

The `NowPlaying` event can appear in several different forms during playback:

1. **Initial playlist notification**: Contains only `listId` but no video information
   ```json
   {"listId":"RQHOSZo8I72PfncOk8TEWlvzMbJFs"}
   ```

2. **Initial video loading**: Contains basic video information but no CPN yet
   ```json
   {"currentTime":"0","duration":"0","listId":"RQHOSZo8I72PfncOk8TEWlvzMbJFs",
    "loadedTime":"0","state":"3","videoId":"dQw4w9WgXcQ"}
   ```

3. **Complete video information**: Contains full information including CPN
   ```json
   {"cpn":"pNuc5Oktxo2_Odby","currentTime":"0.716","duration":"212.061",
    "listId":"RQHOSZo8I72PfncOk8TEWlvzMbJFs","loadedTime":"14.68",
    "seekableEndTime":"212.04","seekableStartTime":"0","state":"1",
    "videoId":"dQw4w9WgXcQ"}
   ```

### StateChange Events

The `StateChange` events contain information about the playback state but do not include the video ID or video metadata. They must be matched with NowPlaying events using the CPN (Client Playback Nonce) to associate them with a specific video.

```json
{"cpn":"pNuc5Oktxo2_Odby","currentTime":"30.248","duration":"212.061",
 "loadedTime":"42.32","seekableEndTime":"212.04","seekableStartTime":"0","state":"1"}
```

StateChange events contain playback information only - timestamps, durations, and state codes (where "1" = playing, "2" = paused, "3" = buffering).

### PlaybackSession Events

This library provides a synthetic `PlaybackSession` event that combines data from both `NowPlaying` and `StateChange` events for the same video (matched by their Client Playback Nonce or CPN). This provides you with a more complete picture of the current playback state:

```rust
LoungeEvent::PlaybackSession(session) => {
    println!("Video: {}", session.video_id);
    println!("Position: {}s / {}s", session.current_time, session.duration);
    println!("State: {}", session.state); // "1" = playing, "2" = paused, "3" = buffering
    
    // List ID is available if the video is part of a playlist
    if let Some(list_id) = &session.list_id {
        println!("Part of playlist: {}", list_id);
    }
}
```

Note that the `video_data` field (containing title, author, etc.) is `None` by default, as this requires a separate API call to populate.

## Examples

The library includes a basic example application to help you understand its usage.

### Basic Example

```bash
cargo run --example basic_example <your_pairing_code>
```

This example demonstrates:
- Pairing with a screen
- Connecting to the device
- Creating event receiver
- Sending commands (play, pause, seek, volume)
- Receiving and handling events

### Debug Mode

You can enable debug mode to see the raw JSON payload of all events:

```rust
// Enable debug mode to see all event data
client.enable_debug_mode();

// Later, when done debugging
client.disable_debug_mode();
```

## API Reference

The library provides the following main components:

### `LoungeClient`

The main client for interacting with the YouTube Lounge API.

#### Methods

- `new(screen_id: &str, lounge_token: &str, device_name: &str) -> Self`
- `with_device_id(screen_id: &str, lounge_token: &str, device_name: &str, device_id: &str) -> Self`
- `device_id(&self) -> &str`
- `event_receiver(&self) -> broadcast::Receiver<LoungeEvent>`
- `enable_debug_mode(&mut self)`
- `disable_debug_mode(&mut self)`
- `pair_with_screen(pairing_code: &str) -> Result<Screen, LoungeError>`
- `refresh_lounge_token(screen_id: &str) -> Result<Screen, LoungeError>`
- `check_screen_availability(&self) -> Result<bool, LoungeError>`
- `check_screen_availability_with_refresh(&mut self) -> Result<bool, LoungeError>`
- `connect(&mut self) -> Result<(), LoungeError>`
- `send_command(&mut self, command: PlaybackCommand) -> Result<(), LoungeError>`
- `send_command_with_refresh(&mut self, command: PlaybackCommand) -> Result<(), LoungeError>`
- `disconnect(&mut self) -> Result<(), LoungeError>`
- `get_thumbnail_url(video_id: &str, thumbnail_idx: u8) -> String`

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
- `SetPlaylist { ... }` - Play a video or playlist
- `AddVideo { ... }` - Add a video to the queue

#### Helper Methods
- `set_playlist(video_id: String) -> Self`
- `set_playlist_by_id(list_id: String) -> Self`
- `set_playlist_with_index(list_id: String, index: i32) -> Self`
- `add_video(video_id: String) -> Self`

### `LoungeEvent`

Events received from the YouTube Lounge API:

- `StateChange(PlaybackState)`
- `NowPlaying(NowPlaying)`
- `LoungeStatus(Vec<Device>, Option<String>)`
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
- `VolumeChanged(VolumeChanged)`
- `Unknown(String)`

### `YoutubeValueParser`

Utility trait for parsing YouTube API string values:

- `parse_float(s: &str) -> f64`
- `parse_int(s: &str) -> i32`
- `parse_bool(s: &str) -> bool`
- `parse_list(s: &str) -> Vec<String>`

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

## License

This project is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License (CC BY-NC 4.0) - see the LICENSE file for details.

Important: This license prohibits any commercial use of this code without explicit permission from the copyright holder.