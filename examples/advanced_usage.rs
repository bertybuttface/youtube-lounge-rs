use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use tokio::time::{sleep, Duration};
use youtube_lounge_rs::{HasVolume, LoungeClient, LoungeEvent, PlaybackCommand, Screen};

// Structure to store authentication data for multiple screens
#[derive(Serialize, Deserialize, Default, Clone)]
struct AuthStore {
    screens: HashMap<String, StoredScreen>,
}

// Screen information to store
#[derive(Serialize, Deserialize, Clone)]
struct StoredScreen {
    name: Option<String>,
    screen_id: String,
    lounge_token: String,
    device_name: String,
    device_id: Option<String>, // Device ID for persistent sessions
}

impl From<&Screen> for StoredScreen {
    fn from(screen: &Screen) -> Self {
        StoredScreen {
            name: screen.name.clone(),
            screen_id: screen.screen_id.clone(),
            lounge_token: screen.lounge_token.clone(),
            device_name: "Rust Lounge Client".to_string(),
            device_id: None, // Will be set after client creation
        }
    }
}

// Load auth data from file
fn load_auth_data() -> AuthStore {
    let auth_file = Path::new("youtube_auth.json");

    if !auth_file.exists() {
        return AuthStore::default();
    }

    match File::open(auth_file) {
        Ok(mut file) => {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(auth_store) = serde_json::from_str(&contents) {
                    return auth_store;
                }
            }
            AuthStore::default()
        }
        Err(_) => AuthStore::default(),
    }
}

// Save auth data to file
fn save_auth_data(auth_store: &AuthStore) -> io::Result<()> {
    let auth_file = Path::new("youtube_auth.json");
    let contents = serde_json::to_string_pretty(auth_store)?;

    let mut file = File::create(auth_file)?;
    file.write_all(contents.as_bytes())?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Process command line arguments
    let args: Vec<String> = env::args().collect();
    let mut pair_mode = false;
    let mut pairing_code = String::new();
    let mut screen_name = String::new();

    // Check if we're in pairing mode
    if args.len() > 1 {
        if args[1] == "help" {
            println!("YouTube Lounge API Advanced Example");
            println!("Usage:");
            println!("  cargo run --example advanced_usage              - Connect to a previously paired screen");
            println!("  cargo run --example advanced_usage pair CODE    - Pair with a new screen using the CODE");
            println!("  cargo run --example advanced_usage debug        - Connect with debug mode to see raw event data");
            println!("  cargo run --example advanced_usage pair CODE debug - Pair with debug mode enabled");
            return Ok(());
        } else if args[1] == "pair" {
            pair_mode = true;

            if args.len() > 2 {
                // Remove any spaces from the pairing code
                pairing_code = args[2].replace(" ", "");
            } else {
                eprintln!(
                    "Usage: cargo run --example advanced_usage pair <pairing_code> [screen_name]"
                );
                return Ok(());
            }
        }
    }

    // Optional screen name for identification
    if args.len() > 3 {
        screen_name = args[3].clone();
    }

    // Load existing authentication data
    let mut auth_store = load_auth_data();

    // Screen to use for the example
    let stored_screen = if pair_mode {
        // Pair with a new screen
        println!("Pairing with screen using code: {}", pairing_code);

        let screen = match LoungeClient::pair_with_screen(&pairing_code).await {
            Ok(screen) => {
                println!("Successfully paired with screen: {:?}", screen);
                screen
            }
            Err(e) => {
                eprintln!("Failed to pair with screen: {}", e);
                return Err(e.into());
            }
        };

        // Create a stored screen
        let mut stored = StoredScreen::from(&screen);

        // Set custom name if provided
        if !screen_name.is_empty() {
            stored.device_name = screen_name;
        }

        // Store the screen information
        auth_store
            .screens
            .insert(stored.screen_id.clone(), stored.clone());

        // Save the updated authentication data
        if let Err(e) = save_auth_data(&auth_store) {
            eprintln!("Warning: Failed to save authentication data: {}", e);
        } else {
            println!("Authentication data saved successfully");
        }

        stored
    } else {
        // Use existing screen if available
        if auth_store.screens.is_empty() {
            eprintln!(
                "No paired screens found. Use 'cargo run --example example pair <code>' to pair."
            );
            return Ok(());
        }

        // Just use the first screen for this example
        // In a real app, you might want to let the user select which screen to use
        let screen_id = auth_store.screens.keys().next().unwrap().clone();
        auth_store.screens.get(&screen_id).unwrap().clone()
    };

    // Create a client with the stored screen info
    let mut client = if let Some(device_id) = &stored_screen.device_id {
        println!("Using persistent device ID: {}", device_id);
        // Use the existing device ID for continuity across sessions
        LoungeClient::with_device_id(
            &stored_screen.screen_id,
            &stored_screen.lounge_token,
            &stored_screen.device_name,
            device_id,
        )
    } else {
        // Generate a new device ID (this is the first connection)
        let client = LoungeClient::new(
            &stored_screen.screen_id,
            &stored_screen.lounge_token,
            &stored_screen.device_name,
        );

        // Save the generated device ID for future sessions
        let device_id = client.device_id().to_string();
        println!("Generated new device ID: {}", device_id);

        // Update our storage with the new device ID
        let mut updated_screen = stored_screen.clone();
        updated_screen.device_id = Some(device_id);

        auth_store
            .screens
            .insert(updated_screen.screen_id.clone(), updated_screen);

        // Save the updated authentication data with device ID
        if let Err(e) = save_auth_data(&auth_store) {
            eprintln!("Warning: Failed to save device ID: {}", e);
        } else {
            println!("Device ID saved successfully for future sessions");
        }

        client
    };

    // Check for debug mode flag
    let debug_mode = args.contains(&"debug".to_string());
    if debug_mode {
        println!("Debug mode enabled - will show raw JSON payloads");
        client.enable_debug_mode();
    }

    // Set a callback to save refreshed tokens
    let auth_store_clone = auth_store.clone();
    client.set_token_refresh_callback(move |screen_id, new_token| {
        println!("Token refreshed for screen {}", screen_id);

        // Clone auth_store again since we need to modify it
        let mut updated_auth = auth_store_clone.clone();

        // Get the screen from the store and update its token
        if let Some(mut screen) = updated_auth.screens.get(screen_id).cloned() {
            screen.lounge_token = new_token.to_string();
            updated_auth.screens.insert(screen_id.to_string(), screen);

            // Save the updated auth data
            match save_auth_data(&updated_auth) {
                Ok(_) => println!("Updated token saved successfully"),
                Err(e) => eprintln!("Failed to save refreshed token: {}", e),
            }
        }
    });

    // Check if the screen is available with automatic token refresh
    println!(
        "Checking if screen '{}' is available...",
        stored_screen.name.as_deref().unwrap_or("Unknown")
    );
    match client.check_screen_availability_with_refresh().await {
        Ok(available) => {
            if available {
                println!("Screen is available");
            } else {
                println!("Screen is not available");
                // If in regular mode and the screen is unavailable, the token might be expired
                if !pair_mode {
                    eprintln!("The token might be expired. Try pairing again with a new code.");
                }
                return Ok(());
            }
        }
        Err(e) => {
            eprintln!("Failed to check screen availability: {}", e);
            // If in regular mode and there's an error, the token might be expired
            if !pair_mode {
                eprintln!("The token might be expired. Try pairing again with a new code.");
            }
            return Err(e.into());
        }
    }

    // First get receivers to listen for events and session updates
    // IMPORTANT: Always subscribe to events BEFORE connecting
    let mut rx = client.event_receiver();
    let mut session_rx = client.session_receiver();

    // Now connect to the screen
    println!("Connecting to screen...");
    match client.connect().await {
        Ok(_) => println!("Connected to screen"),
        Err(e) => {
            eprintln!("Failed to connect to screen: {}", e);
            return Err(e.into());
        }
    }

    // Spawn a task to handle events
    let event_handle = tokio::spawn(async move {
        // Create a separate task for session events
        let _session_handle = tokio::spawn(async move {
            loop {
                match session_rx.recv().await {
                    Ok(session) => {
                        println!("\n=== PlaybackSession Update ===");
                        println!(
                            "  Video ID: {}",
                            session.video_id.as_deref().unwrap_or("Unknown")
                        );
                        println!("  CPN: {}", session.cpn);
                        println!(
                            "  Progress: {:.2}/{:.2} ({:.1}%)",
                            session.current_time,
                            session.duration,
                            session.progress_percentage()
                        );
                        println!("  State: {} ({})", session.state_name(), session.state);

                        if let Some(list_id) = &session.list_id {
                            println!("  Playlist: {}", list_id);
                        }

                        if let Some(device_id) = &session.device_id {
                            println!("  Device ID: {}", device_id);
                        }

                        if let Some(history) = &session.video_history {
                            println!("  Video history: {} videos", history.len());
                        }
                        println!("=============================\n");
                    }
                    Err(e) => match e {
                        tokio::sync::broadcast::error::RecvError::Closed => {
                            println!("Session channel closed");
                            break;
                        }
                        tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
                            println!(
                                "Warning: Session receiver lagging, missed {} updates",
                                missed
                            );
                        }
                    },
                }
            }
        });

        // Main event loop
        loop {
            match rx.recv().await {
                Ok(event) => {
                    match event {
                        LoungeEvent::SessionEstablished => {
                            println!("Session established");
                        }
                        LoungeEvent::StateChange(state) => {
                            println!("Playback state changed:");
                            println!("  Video ID: {}", state.video_id);
                            println!(
                                "  Position: {:.2}/{:.2}",
                                state.current_time_value(),
                                state.duration_value()
                            );
                            println!("  State: {} ({})", state.state_name(), state.state);
                            println!(
                                "  Volume: {} - Muted: {}",
                                state.volume_value(),
                                state.is_muted()
                            );
                            if let Some(cpn) = &state.cpn {
                                println!("  CPN: {}", cpn);
                            }
                        }
                        LoungeEvent::NowPlaying(now_playing) => {
                            println!("Now playing:");
                            println!("  Video ID: {}", now_playing.video_id);
                            println!("  Duration: {:.2}", now_playing.duration_value());
                            println!("  Current time: {:.2}", now_playing.current_time_value());
                            if let Some(list_id) = &now_playing.list_id {
                                println!("  Playlist: {}", list_id);
                            }
                            if let Some(cpn) = &now_playing.cpn {
                                println!("  CPN: {}", cpn);
                            }
                            if let Some(video_list) =
                                &now_playing.mdx_expanded_receiver_video_id_list
                            {
                                println!("  Video history: {}", video_list);
                            }
                        }
                        LoungeEvent::LoungeStatus(devices, queue_id) => {
                            println!("Lounge status update - Connected devices:");
                            for device in devices {
                                println!("  Device: {} ({})", device.name, device.device_type);
                                if let Some(info) = device.device_info {
                                    println!(
                                        "    Brand: {}, Model: {}, Type: {}",
                                        info.brand, info.model, info.device_type
                                    );
                                }
                            }

                            // Display the queue ID if available
                            if let Some(id) = queue_id {
                                println!("  Queue ID: {}", id);
                            }
                        }
                        LoungeEvent::ScreenDisconnected => {
                            println!("Screen disconnected");
                            break;
                        }
                        LoungeEvent::Unknown(event_info) => {
                            println!("======= UNKNOWN EVENT =======");
                            println!("{}", event_info);
                            println!("=============================");
                        }
                        LoungeEvent::AdStateChange(ad_state) => {
                            println!("Ad state change:");
                            println!("  Content video ID: {}", ad_state.content_video_id);
                            println!("  Skip enabled: {}", ad_state.is_skip_enabled);
                        }
                        LoungeEvent::SubtitlesTrackChanged(track) => {
                            println!("Subtitles track changed:");
                            println!("  Video ID: {}", track.video_id);
                        }
                        LoungeEvent::AutoplayModeChanged(mode) => {
                            println!("Autoplay mode changed:");
                            println!("  Mode: {}", mode.autoplay_mode);
                        }
                        LoungeEvent::HasPreviousNextChanged(nav) => {
                            println!("Navigation state changed:");
                            println!("  Has next: {}", nav.has_next);
                            println!("  Has previous: {}", nav.has_previous);
                        }
                        LoungeEvent::VideoQualityChanged(quality) => {
                            println!("Video quality changed:");
                            println!("  Quality level: {}", quality.quality_level);
                            println!("  Available levels: {}", quality.available_quality_levels);
                            println!("  Video ID: {}", quality.video_id);
                        }
                        LoungeEvent::AudioTrackChanged(audio) => {
                            println!("Audio track changed:");
                            println!("  Audio track ID: {}", audio.audio_track_id);
                            println!("  Video ID: {}", audio.video_id);
                        }
                        LoungeEvent::PlaylistModified(playlist) => {
                            println!("Playlist modified:");
                            println!("  Current index: {:?}", playlist.current_index);
                            println!("  First video ID: {}", playlist.first_video_id);
                            println!("  List ID: {}", playlist.list_id);
                            println!("  Video ID: {}", playlist.video_id);
                        }
                        LoungeEvent::AutoplayUpNext(next) => {
                            println!("Autoplay up next:");
                            println!("  Video ID: {}", next.video_id);
                        }
                        LoungeEvent::VolumeChanged(volume) => {
                            println!("Volume changed:");
                            println!("  Volume level: {}", volume.volume_level());
                            println!("  Muted: {}", volume.is_muted());
                        }
                    }
                }
                Err(e) => match e {
                    tokio::sync::broadcast::error::RecvError::Closed => {
                        println!("Event channel closed");
                        break;
                    }
                    tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
                        println!(
                            "Warning: Event receiver lagging behind, missed {} events",
                            missed
                        );
                    }
                },
            }
        }
    });

    // Send commands to control playback
    println!("Playing a YouTube video...");
    // Use an actual video ID or one that works with your YouTube device
    // This is just an example (Rick Astley - Never Gonna Give You Up)
    let video_id = "dQw4w9WgXcQ";

    // Play a specific video with automatic token refresh
    match client
        .send_command_with_refresh(PlaybackCommand::set_playlist(video_id.to_string()))
        .await
    {
        Ok(_) => println!("Started playback of video: {}", video_id),
        Err(e) => eprintln!("Failed to start playback: {}", e),
    }

    // Wait a bit to let the video start
    sleep(Duration::from_secs(2)).await;

    // Pause the video with automatic token refresh
    match client
        .send_command_with_refresh(PlaybackCommand::Pause)
        .await
    {
        Ok(_) => println!("Paused playback"),
        Err(e) => eprintln!("Failed to pause: {}", e),
    }

    sleep(Duration::from_secs(2)).await;

    // Resume playback with automatic token refresh
    match client
        .send_command_with_refresh(PlaybackCommand::Play)
        .await
    {
        Ok(_) => println!("Resumed playback"),
        Err(e) => eprintln!("Failed to resume: {}", e),
    }

    sleep(Duration::from_secs(2)).await;

    // Seek to 30 seconds with automatic token refresh
    match client
        .send_command_with_refresh(PlaybackCommand::SeekTo { new_time: 30.0 })
        .await
    {
        Ok(_) => println!("Seeked to 30 seconds"),
        Err(e) => eprintln!("Failed to seek: {}", e),
    }

    sleep(Duration::from_secs(2)).await;

    // Adjust volume with automatic token refresh
    match client
        .send_command_with_refresh(PlaybackCommand::SetVolume { volume: 50 })
        .await
    {
        Ok(_) => println!("Set volume to 50%"),
        Err(e) => eprintln!("Failed to set volume: {}", e),
    }

    // Wait a bit between commands
    sleep(Duration::from_secs(2)).await;

    // Mute the volume
    println!("Muting audio...");
    match client
        .send_command_with_refresh(PlaybackCommand::Mute)
        .await
    {
        Ok(_) => println!("Audio muted"),
        Err(e) => eprintln!("Failed to mute: {}", e),
    }

    sleep(Duration::from_secs(2)).await;

    // Unmute the volume
    println!("Unmuting audio...");
    match client
        .send_command_with_refresh(PlaybackCommand::Unmute)
        .await
    {
        Ok(_) => println!("Audio unmuted"),
        Err(e) => eprintln!("Failed to unmute: {}", e),
    }

    sleep(Duration::from_secs(2)).await;

    // Demonstrate the AddVideo command - add another video to the queue
    println!("\nDemonstrating queue functionality...");
    println!("Adding a video to the queue (will play after current video)");
    // Use a different video for demonstration
    let queue_video_id = "QH2-TGUlwu4"; // Nyan Cat
    match client
        .send_command_with_refresh(PlaybackCommand::add_video(queue_video_id.to_string()))
        .await
    {
        Ok(_) => println!("Successfully added video {} to queue", queue_video_id),
        Err(e) => eprintln!("Failed to add video to queue: {}", e),
    }

    // Keeping the connection alive for a while - wait a bit longer
    // to see if we can observe events from YouTube client actions
    println!("\nNow waiting for 20 seconds - please perform actions in the YouTube client");
    println!("Try playing, pausing, seeking, etc. directly on your device");
    println!("Watch for event updates and PlaybackSession updates in the output\n");

    // Add a demonstration of session query capabilities
    sleep(Duration::from_secs(10)).await;

    println!("\n=== Session Query Demonstration ===");
    // Get the current session if it exists
    if let Some(session) = client.get_current_session() {
        println!("There is currently 1 active session");

        let cpn = &session.cpn;
        println!("Querying session with CPN: {}", cpn);
        if let Some(session) = client.get_session_by_cpn(cpn) {
            println!(
                "  Found session: {} in state {}",
                session.video_id.as_deref().unwrap_or("Unknown"),
                session.state_name()
            );
        }
    } else {
        println!("No active session found");
    }

    sleep(Duration::from_secs(10)).await;

    // Disconnect from the screen
    println!("Disconnecting from screen...");
    match client.disconnect().await {
        Ok(_) => println!("Disconnected from screen"),
        Err(e) => eprintln!("Failed to disconnect: {}", e),
    }

    // Abort the event handler to ensure clean exit
    event_handle.abort();
    println!("Event handler aborted, example complete.");

    // Display any current sessions before exiting
    println!("\nFinal session state before exit:");
    if let Some(current_session) = client.get_current_session() {
        println!(
            "  Currently playing: {}",
            current_session.video_id.as_ref().map_or("Unknown", |id| id)
        );
        println!(
            "  At position: {:.2}/{:.2}",
            current_session.current_time, current_session.duration
        );
        println!("  State: {}", current_session.state_name());

        if let Some(device_id) = &current_session.device_id {
            println!("  Device ID: {}", device_id);
        }
    } else {
        println!("  No active session");
    }

    Ok(())
}
