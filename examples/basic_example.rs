use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use tokio::time::{sleep, Duration};
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
}

impl From<&Screen> for StoredScreen {
    fn from(screen: &Screen) -> Self {
        StoredScreen {
            name: screen.name.clone(),
            screen_id: screen.screen_id.clone(),
            lounge_token: screen.lounge_token.clone(),
            device_id: String::new(), // Will be set after client creation
            device_name: None,
        }
    }
}

const AUTH_FILENAME: &str = "youtube_auth.json";

/// A simple example showing how to use the YouTube Lounge API with persistence
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut pairing_code = None;
    let mut debug_mode = false;

    // Simple argument parsing
    for i in 1..args.len() {
        if args[i] == "--debug" || args[i] == "-d" {
            debug_mode = true;
        } else if pairing_code.is_none() {
            pairing_code = Some(args[i].clone());
        }
    }

    // Create a client - either with stored auth or by pairing
    let mut client = if pairing_code.is_some() {
        println!(
            "Pairing with new screen using code: {}",
            pairing_code.as_ref().unwrap()
        );
        create_client_with_pairing(pairing_code.as_ref().unwrap()).await?
    } else {
        match create_client_from_stored_auth().await {
            Ok(client) => {
                println!("Using stored authentication data");
                client
            }
            Err(_) => {
                println!("Usage: basic_example [pairing_code] [--debug/-d]");
                println!("  - pairing_code: Code shown on your YouTube TV screen");
                println!("  - --debug or -d: Enable verbose debug logging");
                return Ok(());
            }
        }
    };

    println!("Using device ID: {}", client.device_id());

    // Only enable debug mode if requested
    if debug_mode {
        client.enable_debug_mode();
        println!("Debug mode enabled - will show raw JSON payloads for events");
    }

    // Step 3: Subscribe to events before connecting
    let mut receiver = client.event_receiver();

    // Step 4: Check if the screen is available (with automatic token refresh if needed)
    let available = client.check_screen_availability_with_refresh().await?;
    println!("Screen available: {}", available);

    if !available {
        println!("Screen is not available, cannot connect");
        return Ok(());
    }

    // Step 5: Connect to the screen
    println!("Connecting to screen...");
    client.connect().await?;
    println!("Connected to screen");

    // Step 6: Handle events in a separate task
    println!("Starting event handling");

    // Create a separate async task to receive events
    let _event_handler = tokio::spawn(async move {
        println!("Event handler task started");

        while let Ok(event) = receiver.recv().await {
            let now = chrono::Local::now().format("%H:%M:%S.%3f");
            println!("[{}] Received event: {:?}", now, event);
            match event {
                LoungeEvent::NowPlaying(np) => {
                    if let Some(video_data) = &np.video_data {
                        println!("Now playing: {} ({})", video_data.title, np.video_id);
                    } else {
                        println!("Now playing: {}", np.video_id);
                    }
                }
                LoungeEvent::StateChange(state) => {
                    println!("State changed: {}", state.state);
                    println!("Current time: {}s", state.current_time);
                }
                LoungeEvent::PlaybackSession(session) => {
                    if let Some(video_data) = &session.video_data {
                        println!(
                            "Playback Session - {} ({}) - {}s / {}s",
                            video_data.title,
                            session.video_id,
                            session.current_time,
                            session.duration
                        );
                    } else {
                        println!(
                            "Playback Session - {} - {}s / {}s",
                            session.video_id, session.current_time, session.duration
                        );
                    }
                    println!(
                        "  State: {}, Loaded: {}s",
                        session.state, session.loaded_time
                    );
                }
                LoungeEvent::ScreenDisconnected => {
                    println!("Screen disconnected");
                    break;
                }
                LoungeEvent::SessionEstablished => {
                    println!("Session established - ready to send commands");
                }
                LoungeEvent::AdStateChange(state) => {
                    println!(
                        "Ad state changed - Content video: {}, Skippable: {}",
                        state.content_video_id,
                        state.is_skippable()
                    );
                }
                LoungeEvent::SubtitlesTrackChanged(state) => {
                    println!("Subtitles track changed for video: {}", state.video_id);
                }
                LoungeEvent::AudioTrackChanged(state) => {
                    println!(
                        "Audio track changed to: {} for video: {}",
                        state.audio_track_id, state.video_id
                    );
                }
                LoungeEvent::AutoplayModeChanged(state) => {
                    println!("Autoplay mode changed to: {}", state.autoplay_mode);
                }
                LoungeEvent::HasPreviousNextChanged(state) => {
                    println!(
                        "Navigation state changed - Has next: {}, Has previous: {}",
                        state.has_next(),
                        state.has_previous()
                    );
                }
                LoungeEvent::VideoQualityChanged(state) => {
                    println!(
                        "Video quality changed to: {} for video: {}",
                        state.quality_level, state.video_id
                    );
                    println!("Available qualities: {:?}", state.available_qualities());
                }
                LoungeEvent::VolumeChanged(state) => {
                    println!(
                        "Volume changed to: {}, Muted: {}",
                        state.volume_level(),
                        state.is_muted()
                    );
                }
                LoungeEvent::PlaylistModified(state) => {
                    println!(
                        "Playlist modified - List ID: {}, Video ID: {}",
                        state.list_id, state.video_id
                    );
                    if let Some(idx) = state.current_index_value() {
                        println!("Current Index: {}", idx);
                    }
                }
                LoungeEvent::AutoplayUpNext(state) => {
                    println!("Autoplay up next: {}", state.video_id);
                }
                LoungeEvent::LoungeStatus(devices, queue_id) => {
                    println!("Lounge status - {} devices connected", devices.len());
                    if let Some(qid) = queue_id {
                        println!("Queue ID: {}", qid);
                    }
                    for device in devices {
                        println!("  Device: {} ({})", device.name, device.device_type);
                        if let Some(info) = &device.device_info {
                            println!("    Brand: {}, Model: {}", info.brand, info.model);
                        }
                    }
                }
                LoungeEvent::Unknown(event_info) => {
                    println!("Unknown event: {}", event_info);
                }
            }
        }
    });

    // Wait a moment for connection to stabilize
    sleep(Duration::from_secs(1)).await;

    // Step 7: Send commands to control playback

    // Play a specific video
    println!("Starting a video...");
    client
        .send_command_with_refresh(
            PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string()), // Rick Astley
        )
        .await?;

    // Wait for video to start
    sleep(Duration::from_secs(3)).await;

    // Pause the video
    println!("Pausing...");
    client
        .send_command_with_refresh(PlaybackCommand::Pause)
        .await?;

    sleep(Duration::from_secs(2)).await;

    // Resume playback
    println!("Resuming...");
    client
        .send_command_with_refresh(PlaybackCommand::Play)
        .await?;

    sleep(Duration::from_secs(2)).await;

    // Seek to a specific position
    println!("Seeking to 30 seconds...");
    client
        .send_command_with_refresh(PlaybackCommand::SeekTo { new_time: 30.0 })
        .await?;

    sleep(Duration::from_secs(2)).await;

    // Adjust volume
    println!("Setting volume to 50%...");
    client
        .send_command_with_refresh(PlaybackCommand::SetVolume { volume: 50 })
        .await?;

    // Wait to observe results
    println!("\nWaiting 5 seconds to observe results...");
    sleep(Duration::from_secs(5)).await;

    // Step 8: Disconnect
    println!("Disconnecting...");
    client.disconnect().await?;

    // Give some time for last events to process
    sleep(Duration::from_secs(1)).await;

    // We don't actually need to wait for the event handler to complete
    // as it will terminate when the program exits

    Ok(())
}

// Create client by pairing with a screen
async fn create_client_with_pairing(pairing_code: &str) -> Result<LoungeClient, Box<dyn Error>> {
    // Step 1: Pair with a screen
    let screen = LoungeClient::pair_with_screen(pairing_code).await?;

    println!(
        "Successfully paired with screen: {}",
        screen.name.as_deref().unwrap_or("Unknown")
    );

    // Step 2: Create client
    let client = LoungeClient::new(
        &screen.screen_id,
        &screen.lounge_token,
        "Rust YouTube Controller",
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
    println!("Auth data saved to {} for next time", AUTH_FILENAME);

    Ok(client)
}

// Create client from stored auth
async fn create_client_from_stored_auth() -> Result<LoungeClient, Box<dyn Error>> {
    // Load auth data from file
    let auth_store = load_auth()?;

    // Get stored screen info - use the first screen we find
    let screen_entry = auth_store
        .screens
        .iter()
        .next()
        .ok_or("No stored screens found")?;
    let stored_screen = screen_entry.1;

    // Create client with stored device ID
    let client = LoungeClient::with_device_id(
        &stored_screen.screen_id,
        &stored_screen.lounge_token,
        stored_screen
            .device_name
            .as_deref()
            .unwrap_or("Rust YouTube Controller"),
        &stored_screen.device_id,
    );

    println!(
        "Using stored screen: {}",
        stored_screen.name.as_deref().unwrap_or("Unknown")
    );

    Ok(client)
}

// Save auth data to file
fn save_auth(auth: &AuthStore) -> io::Result<()> {
    let json = serde_json::to_string_pretty(auth)?;
    let mut file = File::create(AUTH_FILENAME)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

// Load auth data from file
fn load_auth() -> io::Result<AuthStore> {
    if !Path::new(AUTH_FILENAME).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Auth file does not exist",
        ));
    }

    let mut file = File::open(AUTH_FILENAME)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let auth: AuthStore = serde_json::from_str(&contents)?;
    Ok(auth)
}
