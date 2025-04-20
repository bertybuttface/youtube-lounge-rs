use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use youtube_lounge_rs::{LoungeClient, LoungeEvent, PlaybackCommand, Screen};

// Structure to store screen authentication data
#[derive(Serialize, Deserialize, Default)]
struct AuthStore {
    screens: HashMap<String, StoredScreen>,
}

// Screen information to store
#[derive(Serialize, Deserialize, Clone)]
struct StoredScreen {
    name: Option<String>,
    screen_id: String,
    lounge_token: String,
    device_id: String,
    device_name: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

impl From<&Screen> for StoredScreen {
    fn from(screen: &Screen) -> Self {
        StoredScreen {
            name: screen.name.clone(),
            screen_id: screen.screen_id.clone(),
            lounge_token: screen.lounge_token.clone(),
            device_id: String::new(), // Will be set after client creation
            device_name: None,
            enabled: true,
        }
    }
}

const AUTH_FILENAME: &str = "youtube_auth.json";

/// A simple example showing how to use the YouTube Lounge API with persistence
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut pairing_code = None;
    let mut log_level = Level::INFO;

    // Simple argument parsing, skip first argument
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--trace" | "-t" => log_level = Level::TRACE,
            "--debug" | "-d" => log_level = Level::DEBUG,
            "--info" | "-i" => log_level = Level::INFO,
            "--warn" | "-w" => log_level = Level::WARN,
            "--error" | "-e" => log_level = Level::ERROR,
            _ if pairing_code.is_none() => pairing_code = Some(arg.clone()),
            _ => {}
        }
    }

    // Initialize tracing subscriber with the specified log level
    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("Logging level set to: {}", log_level);

    // Create a client - either with stored auth or by pairing
    let client = if pairing_code.is_some() {
        info!(
            "Pairing with new screen using code: {}",
            pairing_code.as_ref().unwrap()
        );
        match create_client_with_pairing(pairing_code.as_ref().unwrap()).await {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to pair with screen: {}", e);
                info!(
                    "Make sure the pairing code is correct and your TV is on the pairing screen."
                );
                return Ok(());
            }
        }
    } else {
        match create_client_from_stored_auth().await {
            Ok(client) => {
                info!("Using stored authentication data");
                client
            }
            Err(e) => {
                error!("Failed to initialize client with stored auth: {}", e);
                info!("Usage: basic_example [pairing_code] [log_level]");
                info!("  - pairing_code: Code shown on your YouTube TV screen");
                info!("  - log_level: One of the following:");
                info!("      --trace:  Show all trace-level logs");
                info!("      --debug/-d: Show debug and higher logs");
                info!("      --info/-i:  Show info and higher logs (default)");
                info!("      --warn/-w:  Show warning and higher logs");
                info!("      --error/-e: Show only error logs");
                return Ok(());
            }
        }
    };

    // Store the screen_id for use in logs
    let screen_id = client.screen_id().to_string();

    info!("[{}] Using device ID: {}", screen_id, client.device_id());

    // Step 3: Subscribe to events before connecting
    let mut receiver = client.event_receiver();

    // Step 4: Check if the screen is available (with automatic token refresh if needed)
    match client.check_screen_availability_with_refresh().await {
        Ok(true) => {
            info!(
                "[{}] Screen available, proceeding with connection",
                screen_id
            );
        }
        Ok(false) => {
            error!("[{}] Screen is not available, cannot connect. The screen may be offline or unreachable.", screen_id);
            error!("Check that the YouTube app is open on your TV/device.");
            return Ok(());
        }
        Err(e) => {
            error!("[{}] Failed to check screen availability: {}", screen_id, e);
            error!("Check your network connection and try again.");
            return Ok(());
        }
    }

    // Step 5: Connect to the screen
    info!("[{}] Connecting to screen", screen_id);
    match client.connect().await {
        Ok(_) => info!("[{}] Successfully connected to screen", screen_id),
        Err(e) => {
            error!("[{}] Failed to connect to screen: {}", screen_id, e);
            error!("Check that the screen is still available and the YouTube app is open.");
            return Ok(());
        }
    }

    // Step 6: Handle events in a separate task
    info!("[{}] Starting event handling", screen_id);

    // Create a separate async task to receive events
    let screen_id_clone = screen_id.clone();
    let _event_handler = tokio::spawn(async move {
        info!("[{}] Event handler task started", screen_id_clone);

        while let Ok(event) = receiver.recv().await {
            debug!("Received event: {:?}", event);
            match event {
                LoungeEvent::NowPlaying(np) => {
                    if let Some(video_data) = &np.video_data {
                        info!(
                            "[{}] Now playing: {} ({}) - State: {} ({})",
                            screen_id_clone,
                            video_data.title,
                            np.video_id,
                            np.state,
                            np.status()
                        );
                    } else {
                        info!(
                            "[{}] Now playing: {} - State: {} ({})",
                            screen_id_clone,
                            np.video_id,
                            np.state,
                            np.status()
                        );
                    }
                }
                LoungeEvent::StateChange(state) => {
                    info!(
                        "[{}] State changed: {} ({})",
                        screen_id_clone,
                        state.state,
                        state.status()
                    );
                    debug!(
                        "[{}] Current time: {}s",
                        screen_id_clone, state.current_time
                    );
                }
                LoungeEvent::PlaybackSession(session) => {
                    // Calculate progress percentage
                    let progress_pct = if session.duration > 0.0 {
                        (session.current_time / session.duration * 100.0).round() as i32
                    } else {
                        0
                    };

                    if let Some(video_data) = &session.video_data {
                        info!(
                            "[{}] Playback Session - {} ({}) - {}s / {}s [{}%] - State: {} ({})",
                            screen_id_clone,
                            video_data.title,
                            session.video_id,
                            session.current_time,
                            session.duration,
                            progress_pct,
                            session.state,
                            session.status()
                        );
                    } else {
                        info!(
                            "[{}] Playback Session - {} - {}s / {}s [{}%] - State: {} ({})",
                            screen_id_clone,
                            session.video_id,
                            session.current_time,
                            session.duration,
                            progress_pct,
                            session.state,
                            session.status()
                        );
                    }
                    debug!(
                        "[{}]   State: {} ({}), Loaded: {}s",
                        screen_id_clone,
                        session.state,
                        session.status(),
                        session.loaded_time
                    );
                }
                LoungeEvent::ScreenDisconnected => {
                    warn!("[{}] Screen disconnected", screen_id_clone);
                    break;
                }
                LoungeEvent::SessionEstablished => {
                    info!(
                        "[{}] Session established - ready to send commands",
                        screen_id_clone
                    );
                }
                LoungeEvent::AdStateChange(state) => {
                    info!(
                        "[{}] Ad state changed - Content video: {}, Skippable: {}",
                        screen_id_clone,
                        state.content_video_id,
                        state.is_skippable()
                    );
                }
                LoungeEvent::SubtitlesTrackChanged(state) => {
                    info!(
                        "[{}] Subtitles track changed for video: {}",
                        screen_id_clone, state.video_id
                    );
                }
                LoungeEvent::AudioTrackChanged(state) => {
                    info!(
                        "[{}] Audio track changed to: {} for video: {}",
                        screen_id_clone, state.audio_track_id, state.video_id
                    );
                }
                LoungeEvent::AutoplayModeChanged(state) => {
                    info!(
                        "[{}] Autoplay mode changed to: {}",
                        screen_id_clone, state.autoplay_mode
                    );
                }
                LoungeEvent::HasPreviousNextChanged(state) => {
                    info!(
                        "[{}] Navigation state changed - Has next: {}, Has previous: {}",
                        screen_id_clone,
                        state.has_next(),
                        state.has_previous()
                    );
                }
                LoungeEvent::VideoQualityChanged(state) => {
                    info!(
                        "[{}] Video quality changed to: {} for video: {}",
                        screen_id_clone, state.quality_level, state.video_id
                    );
                    debug!(
                        "[{}] Available qualities: {:?}",
                        screen_id_clone,
                        state.available_qualities()
                    );
                }
                LoungeEvent::VolumeChanged(state) => {
                    info!(
                        "[{}] Volume changed to: {}, Muted: {}",
                        screen_id_clone,
                        state.volume_level(),
                        state.is_muted()
                    );
                }
                LoungeEvent::PlaylistModified(state) => {
                    info!(
                        "[{}] Playlist modified - List ID: {}, Video ID: {}",
                        screen_id_clone, state.list_id, state.video_id
                    );
                    if let Some(idx) = state.current_index_value() {
                        debug!("[{}] Current Index: {}", screen_id_clone, idx);
                    }
                }
                LoungeEvent::PlaylistModeChanged(state) => {
                    info!(
                        "[{}] Playlist mode changed - LoopEnabled: {}, ShuffleEnabled: {}",
                        screen_id_clone, state.loop_enabled, state.shuffle_enabled
                    );
                }
                LoungeEvent::AutoplayUpNext(state) => {
                    info!("[{}] Autoplay up next: {}", screen_id_clone, state.video_id);
                }
                LoungeEvent::LoungeStatus(devices, queue_id) => {
                    info!(
                        "[{}] Lounge status - {} devices connected",
                        screen_id_clone,
                        devices.len()
                    );
                    if let Some(qid) = queue_id {
                        debug!("[{}] Queue ID: {}", screen_id_clone, qid);
                    }
                    for device in devices {
                        info!(
                            "[{}]   Device: {} ({})",
                            screen_id_clone, device.name, device.device_type
                        );
                        if let Some(info) = &device.device_info {
                            debug!(
                                "[{}]     Brand: {}, Model: {}",
                                screen_id_clone, info.brand, info.model
                            );
                        }
                    }
                }
                LoungeEvent::Unknown(event_info) => {
                    warn!("[{}] Unknown event: {}", screen_id_clone, event_info);
                }
            }
        }
    });

    // Wait a moment for connection to stabilize
    sleep(Duration::from_secs(1)).await;

    // Step 7: Send commands to control playback

    // Play a specific video
    info!("[{}] Starting a video...", screen_id);
    client
        .send_command_with_refresh(
            PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string()), // Rick Astley
        )
        .await?;

    // Wait for video to start
    sleep(Duration::from_secs(3)).await;

    // Pause the video
    info!("[{}] Pausing...", screen_id);
    client
        .send_command_with_refresh(PlaybackCommand::Pause)
        .await?;

    sleep(Duration::from_secs(2)).await;

    // Resume playback
    info!("[{}] Resuming...", screen_id);
    client
        .send_command_with_refresh(PlaybackCommand::Play)
        .await?;

    sleep(Duration::from_secs(2)).await;

    // Seek to a specific position
    info!("[{}] Seeking to 30 seconds...", screen_id);
    client
        .send_command_with_refresh(PlaybackCommand::SeekTo { new_time: 30.0 })
        .await?;

    sleep(Duration::from_secs(2)).await;

    // Adjust volume
    info!("[{}] Setting volume to 50%...", screen_id);
    client
        .send_command_with_refresh(PlaybackCommand::SetVolume { volume: 50 })
        .await?;

    // Wait to observe results
    info!("[{}] Waiting 5 seconds to observe results...", screen_id);
    sleep(Duration::from_secs(5)).await;

    // Step 8: Disconnect
    info!("[{}] Disconnecting...", screen_id);
    if let Err(e) = client.disconnect().await {
        error!("[{}] Error during disconnect: {}", screen_id, e);
    }

    // Give some time for last events to process
    sleep(Duration::from_secs(1)).await;

    // We don't actually need to wait for the event handler to complete
    // as it will terminate when the program exits

    Ok::<(), Box<dyn Error + Send + Sync>>(())
}

// Create client by pairing with a screen
async fn create_client_with_pairing(
    pairing_code: &str,
) -> Result<LoungeClient, Box<dyn Error + Send + Sync>> {
    // Step 1: Pair with a screen
    let screen = LoungeClient::pair_with_screen(pairing_code).await?;

    info!(
        "Successfully paired with screen: {}",
        screen.name.as_deref().unwrap_or("Unknown")
    );

    // Step 2: Create client
    let client = LoungeClient::new(
        &screen.screen_id,
        &screen.lounge_token,
        "Rust YouTube Controller",
        None,
        None, // Explicitly pass None to use the default client
    );

    // Store auth data for next time
    let mut auth_store = load_auth().unwrap_or_default();
    let mut stored_screen = StoredScreen::from(&screen);

    // Set the device ID from client
    stored_screen.device_id = client.device_id().to_string();
    stored_screen.device_name = Some("Rust YouTube Controller".to_string());

    // Add the screen to the map using screen_id as key
    auth_store
        .screens
        .insert(screen.screen_id.clone(), stored_screen);

    // Save to disk
    save_auth(&auth_store)?;
    debug!("Auth data saved to {} for next time", AUTH_FILENAME);

    Ok(client)
}

// Create client from stored auth
async fn create_client_from_stored_auth() -> Result<LoungeClient, Box<dyn Error + Send + Sync>> {
    // Load auth data from file
    let auth_store = match load_auth() {
        Ok(store) => store,
        Err(e) => {
            error!("Failed to load authentication data: {}", e);
            return Err(Box::new(e));
        }
    };

    // Get stored screen info - use the first screen we find
    if auth_store.screens.is_empty() {
        let err = io::Error::new(
            io::ErrorKind::InvalidData,
            "Auth file exists but contains no screens",
        );
        error!("No screens found in auth file");
        return Err(Box::new(err));
    }

    let screen_entry = auth_store.screens.iter().next().unwrap();
    let stored_screen = screen_entry.1;

    // Check if screen data looks valid
    if stored_screen.screen_id.trim().is_empty() {
        let err = io::Error::new(io::ErrorKind::InvalidData, "Stored screen_id is empty");
        error!("Invalid screen data in auth file: empty screen_id");
        return Err(Box::new(err));
    }

    if stored_screen.lounge_token.trim().is_empty() {
        let err = io::Error::new(io::ErrorKind::InvalidData, "Stored lounge_token is empty");
        error!("Invalid screen data in auth file: empty lounge_token");
        return Err(Box::new(err));
    }

    if stored_screen.device_id.trim().is_empty() {
        let err = io::Error::new(io::ErrorKind::InvalidData, "Stored device_id is empty");
        error!("Invalid screen data in auth file: empty device_id");
        return Err(Box::new(err));
    }

    // Create client with stored device ID
    let client = LoungeClient::new(
        &stored_screen.screen_id,
        &stored_screen.lounge_token,
        stored_screen
            .device_name
            .as_deref()
            .unwrap_or("Rust YouTube Controller"),
        Some(stored_screen.device_id.as_str()),
        None, // Explicitly pass None to use the default client
    );

    info!(
        "Using stored screen: {} (ID: {})",
        stored_screen.name.as_deref().unwrap_or("Unknown"),
        stored_screen.screen_id
    );
    debug!("Using lounge token: {}", stored_screen.lounge_token);
    debug!("Using device ID: {}", stored_screen.device_id);

    Ok(client)
}

// Save auth data to file
fn save_auth(auth: &AuthStore) -> io::Result<()> {
    // Generate JSON
    let json = match serde_json::to_string_pretty(auth) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize auth data: {}", e);
            return Err(io::Error::new(io::ErrorKind::InvalidData, e));
        }
    };

    debug!("Serialized auth data to JSON, size: {} bytes", json.len());

    // Create file
    let mut file = match File::create(AUTH_FILENAME) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to create auth file '{}': {}", AUTH_FILENAME, e);
            return Err(e);
        }
    };

    // Write data
    match file.write_all(json.as_bytes()) {
        Ok(_) => {
            info!("Successfully saved auth data to '{}'", AUTH_FILENAME);
            Ok(())
        }
        Err(e) => {
            error!(
                "Failed to write data to auth file '{}': {}",
                AUTH_FILENAME, e
            );
            Err(e)
        }
    }
}

// Load auth data from file
fn load_auth() -> io::Result<AuthStore> {
    let path = Path::new(AUTH_FILENAME);
    if !path.exists() {
        warn!("Auth file '{}' does not exist", AUTH_FILENAME);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Auth file '{}' does not exist", AUTH_FILENAME),
        ));
    }

    // Check if file is readable
    if let Ok(metadata) = path.metadata() {
        debug!("Auth file size: {} bytes", metadata.len());
    } else {
        error!("Cannot read metadata for auth file '{}'", AUTH_FILENAME);
    }

    // Open and read file
    let mut file = match File::open(AUTH_FILENAME) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open auth file '{}': {}", AUTH_FILENAME, e);
            return Err(e);
        }
    };

    let mut contents = String::new();
    match file.read_to_string(&mut contents) {
        Ok(bytes) => debug!("Read {} bytes from auth file", bytes),
        Err(e) => {
            error!("Failed to read auth file '{}': {}", AUTH_FILENAME, e);
            return Err(e);
        }
    }

    // Parse JSON
    match serde_json::from_str(&contents) {
        Ok(auth) => {
            debug!("Successfully parsed auth data from '{}'", AUTH_FILENAME);
            Ok(auth)
        }
        Err(e) => {
            error!("Failed to parse auth file '{}': {}", AUTH_FILENAME, e);
            // Preview the file content for debugging
            if contents.len() > 100 {
                error!("File content preview: {}...", &contents[..100]);
            } else {
                error!("File content: {}", contents);
            }
            Err(io::Error::new(io::ErrorKind::InvalidData, e))
        }
    }
}
