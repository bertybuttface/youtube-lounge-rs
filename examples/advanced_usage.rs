use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use tokio::time::{sleep, Duration};
use youtube_lounge_rs::{LoungeClient, LoungeEvent, PlaybackCommand, Screen};

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
}

impl From<&Screen> for StoredScreen {
    fn from(screen: &Screen) -> Self {
        StoredScreen {
            name: screen.name.clone(),
            screen_id: screen.screen_id.clone(),
            lounge_token: screen.lounge_token.clone(),
            device_name: "Rust Lounge Client".to_string(),
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
    if args.len() > 1 && args[1] == "pair" {
        pair_mode = true;

        if args.len() > 2 {
            // Remove any spaces from the pairing code
            pairing_code = args[2].replace(" ", "");
        } else {
            eprintln!("Usage: cargo run --example example pair <pairing_code> [screen_name]");
            return Ok(());
        }

        // Optional screen name for identification
        if args.len() > 3 {
            screen_name = args[3].clone();
        }
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
    let mut client = LoungeClient::new(
        &stored_screen.screen_id,
        &stored_screen.lounge_token,
        &stored_screen.device_name,
    );

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

    // Connect to the screen
    println!("Connecting to screen...");
    match client.connect().await {
        Ok(_) => println!("Connected to screen"),
        Err(e) => {
            eprintln!("Failed to connect to screen: {}", e);
            return Err(e.into());
        }
    }

    // Get a receiver to listen for events
    let mut rx = client.event_receiver();

    // Spawn a task to handle events
    let event_handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => match event {
                    LoungeEvent::SessionEstablished => {
                        println!("Session established");
                    }
                    LoungeEvent::StateChange(state) => {
                        println!("Playback state changed:");
                        println!(
                            "  Video: {} - {}",
                            state.video_data.title, state.video_data.author
                        );
                        println!(
                            "  Position: {:.2}/{:.2}",
                            state.current_time, state.duration
                        );
                        println!("  State: {}", state.state_name());
                    }
                    LoungeEvent::NowPlaying(now_playing) => {
                        println!("Now playing:");
                        println!(
                            "  Video: {} - {}",
                            now_playing.video_data.title, now_playing.video_data.author
                        );
                        println!("  Video ID: {}", now_playing.video_id);
                        if let Some(list_id) = now_playing.list_id {
                            println!("  Playlist: {}", list_id);
                        }
                    }
                    LoungeEvent::LoungeStatus(devices) => {
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
                        println!("  Current index: {}", playlist.current_index);
                        println!("  First video ID: {}", playlist.first_video_id);
                        println!("  List ID: {}", playlist.list_id);
                        println!("  Video ID: {}", playlist.video_id);
                    }
                },
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
        .send_command_with_refresh(PlaybackCommand::SetPlaylist {
            video_id: video_id.to_string(),
        })
        .await
    {
        Ok(_) => println!("Started playback of video: {}", video_id),
        Err(e) => eprintln!("Failed to start playback: {}", e),
    }

    // Wait a bit to let the video start
    sleep(Duration::from_secs(5)).await;

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

    // Keeping the connection alive for a while
    println!("Keeping connection alive for 10 seconds...");
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

    Ok(())
}
