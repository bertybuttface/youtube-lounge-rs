# YouTube Lounge API Client

A Rust client library for the YouTube Lounge API, which allows controlling YouTube playback on TV devices and other connected displays.

## Features

- Pair with YouTube-enabled TVs and devices using pairing codes
- Control playback (play, pause, volume, seek, etc.)
- Receive real-time playback status updates
- Queue and manage videos for playback
- Handle reconnection and token refresh logic

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
youtube-lounge-rs = "0.1.0"
```

[![Crates.io](https://img.shields.io/crates/v/youtube-lounge-rs.svg)](https://crates.io/crates/youtube-lounge-rs)
[![Docs.rs](https://docs.rs/youtube-lounge-rs/badge.svg)](https://docs.rs/youtube-lounge-rs)
[![CI](https://github.com/bertybuttface/youtube-lounge-rs/workflows/CI/badge.svg)](https://github.com/bertybuttface/youtube-lounge-rs/actions/workflows/ci.yml)
[![License: CC BY-NC 4.0](https://img.shields.io/badge/License-CC%20BY--NC%204.0-lightgrey.svg)](https://creativecommons.org/licenses/by-nc/4.0/)

## Usage

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
// Play a specific video
client.send_command(PlaybackCommand::SetPlaylist { 
    video_id: "dQw4w9WgXcQ".to_string() 
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

### Basic Usage Example

The library includes a basic example that demonstrates the core functionality:

```bash
cargo run --example basic_usage <your_pairing_code>
```

This simple example shows:
- Pairing with a screen
- Connecting to the device
- Sending commands (play, pause, seek, volume)
- Receiving and handling events

### Advanced Example with Persistence

The library also includes an advanced example with session persistence:

```bash
# First time: pair with a screen
cargo run --example advanced_usage pair <your_pairing_code>

# Subsequent runs: reuse stored authentication
cargo run --example advanced_usage
```

Advanced example features:
- **Persistent Authentication**: Stores screen information in a JSON file
- **Multi-Device Support**: Can store and manage multiple paired screens
- **Command-Line Interface**: Supports different modes via command-line arguments
- **Token Validation**: Automatically detects invalid/expired tokens
- **Comprehensive Event Handling**: Displays all events received from the TV

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

### `PlaybackCommand`

Commands that can be sent to control playback:

- `Play`
- `Pause`
- `Next`
- `Previous`
- `SkipAd`
- `SetPlaylist { video_id: String }`
- `SeekTo { new_time: f64 }`
- `SetAutoplayMode { autoplay_mode: String }`
- `SetVolume { volume: i32 }`

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