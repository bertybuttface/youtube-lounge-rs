use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use tokio::time::{sleep, Duration};
use youtube_lounge_rs::{
    HasDuration, HasVolume, LoungeClient, LoungeEvent, PlaybackCommand, Screen,
};

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
    // Structure to hold client state
    struct ClientHandler {
        client: LoungeClient,
        screen_name: String,
        task_handle: Option<tokio::task::JoinHandle<()>>,
    }

    // Function to connect to a screen and create handlers for its events
    async fn connect_to_screen(
        screen: &StoredScreen,
        debug_mode: bool,
        auth_store: AuthStore,
    ) -> Result<ClientHandler, Box<dyn std::error::Error>> {
        // Create a descriptive name for this connection
        let screen_name = screen
            .name
            .as_ref()
            .map(|name| name.clone())
            .unwrap_or_else(|| format!("Screen-{}", &screen.screen_id[..8]));

        println!("[{}] Creating client...", screen_name);

        // Create a client with the stored screen info
        let mut client = if let Some(device_id) = &screen.device_id {
            println!(
                "[{}] Using persistent device ID: {}",
                screen_name, device_id
            );
            // Use the existing device ID for continuity across sessions
            LoungeClient::with_device_id(
                &screen.screen_id,
                &screen.lounge_token,
                &screen.device_name,
                device_id,
            )
        } else {
            // Generate a new device ID (this is the first connection)
            let client =
                LoungeClient::new(&screen.screen_id, &screen.lounge_token, &screen.device_name);

            // Save the generated device ID for future sessions
            let device_id = client.device_id().to_string();
            println!("[{}] Generated new device ID: {}", screen_name, device_id);

            client
        };

        // Enable debug mode if requested
        if debug_mode {
            println!("[{}] Debug mode enabled", screen_name);
            client.enable_debug_mode();
        }

        // Set token refresh callback
        // We don't need to clone the screen_id since we're using it directly from the screen parameter
        let screen_name_clone = screen_name.clone();
        let auth_store_clone = auth_store.clone();
        client.set_token_refresh_callback(move |screen_id, new_token| {
            println!("[{}] Token refreshed", screen_name_clone);

            // Clone auth_store again since we need to modify it
            let mut updated_auth = auth_store_clone.clone();

            // Get the screen from the store and update its token
            if let Some(mut screen) = updated_auth.screens.get(screen_id).cloned() {
                screen.lounge_token = new_token.to_string();
                updated_auth.screens.insert(screen_id.to_string(), screen);

                // Save the updated auth data
                match save_auth_data(&updated_auth) {
                    Ok(_) => println!("[{}] Updated token saved successfully", screen_name_clone),
                    Err(e) => eprintln!(
                        "[{}] Failed to save refreshed token: {}",
                        screen_name_clone, e
                    ),
                }
            }
        });

        // Check if the screen is available with automatic token refresh
        println!("[{}] Checking screen availability...", screen_name);
        match client.check_screen_availability_with_refresh().await {
            Ok(available) => {
                if available {
                    println!("[{}] Screen is available", screen_name);
                } else {
                    println!("[{}] Screen is not available", screen_name);
                    return Err(format!("[{}] Screen is not available", screen_name).into());
                }
            }
            Err(e) => {
                eprintln!(
                    "[{}] Failed to check screen availability: {}",
                    screen_name, e
                );
                return Err(e.into());
            }
        }

        // Get receivers for events and session updates
        // IMPORTANT: Always subscribe to events BEFORE connecting
        let mut rx = client.event_receiver();
        let mut session_rx = client.session_receiver();

        // Connect to the screen
        println!("[{}] Connecting to screen...", screen_name);
        match client.connect().await {
            Ok(_) => println!("[{}] Connected successfully", screen_name),
            Err(e) => {
                eprintln!("[{}] Failed to connect: {}", screen_name, e);
                return Err(e.into());
            }
        }

        // Create a handler task for events
        let screen_name_for_task = screen_name.clone();
        let screen_id_for_task = screen.screen_id.clone();
        let task_handle = tokio::spawn(async move {
            // Create a separate task for session events
            let session_screen_name = screen_name_for_task.clone();
            let _session_handle = tokio::spawn(async move {
                loop {
                    match session_rx.recv().await {
                        Ok(session) => {
                            println!("\n=== [{}] Session Update ===", session_screen_name);
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
                            println!("===============================\n");
                        }
                        Err(e) => match e {
                            tokio::sync::broadcast::error::RecvError::Closed => {
                                println!("[{}] Session channel closed", session_screen_name);
                                break;
                            }
                            tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
                                println!(
                                    "[{}] Warning: Session receiver lagging, missed {} updates",
                                    session_screen_name, missed
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
                                println!(
                                    "[{}] Session established (ID: {})",
                                    screen_name_for_task, screen_id_for_task
                                );
                            }
                            LoungeEvent::StateChange(state) => {
                                println!("[{}] Playback state changed:", screen_name_for_task);
                                println!("  Video ID: {}", state.video_id);
                                println!(
                                    "  Position: {:.2}/{:.2}",
                                    state.current_time(),
                                    state.duration()
                                );
                                println!("  State: {} ({})", state.state_name(), state.state);
                                println!(
                                    "  Volume: {} - Muted: {}",
                                    state.volume(),
                                    state.is_muted()
                                );
                                if let Some(cpn) = &state.cpn {
                                    println!("  CPN: {}", cpn);
                                }
                            }
                            LoungeEvent::NowPlaying(now_playing) => {
                                println!("[{}] Now playing:", screen_name_for_task);
                                println!("  Video ID: {}", now_playing.video_id);
                                println!("  Duration: {:.2}", now_playing.duration());
                                println!("  Current time: {:.2}", now_playing.current_time());
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
                                println!(
                                    "[{}] Lounge status update - Connected devices:",
                                    screen_name_for_task
                                );
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
                                println!("[{}] Screen disconnected", screen_name_for_task);
                                break;
                            }
                            LoungeEvent::Unknown(event_info) => {
                                println!(
                                    "[{}] ======= UNKNOWN EVENT =======",
                                    screen_name_for_task
                                );
                                println!("{}", event_info);
                                println!("=============================");
                            }
                            LoungeEvent::AdStateChange(ad_state) => {
                                println!("[{}] Ad state change:", screen_name_for_task);
                                println!("  Content video ID: {}", ad_state.content_video_id);
                                println!("  Skip enabled: {}", ad_state.is_skip_enabled);
                            }
                            LoungeEvent::SubtitlesTrackChanged(track) => {
                                println!("[{}] Subtitles track changed:", screen_name_for_task);
                                println!("  Video ID: {}", track.video_id);
                            }
                            LoungeEvent::AutoplayModeChanged(mode) => {
                                println!("[{}] Autoplay mode changed:", screen_name_for_task);
                                println!("  Mode: {}", mode.autoplay_mode);
                            }
                            LoungeEvent::HasPreviousNextChanged(nav) => {
                                println!("[{}] Navigation state changed:", screen_name_for_task);
                                println!("  Has next: {}", nav.has_next);
                                println!("  Has previous: {}", nav.has_previous);
                            }
                            LoungeEvent::VideoQualityChanged(quality) => {
                                println!("[{}] Video quality changed:", screen_name_for_task);
                                println!("  Quality level: {}", quality.quality_level);
                                println!(
                                    "  Available levels: {}",
                                    quality.available_quality_levels
                                );
                                println!("  Video ID: {}", quality.video_id);
                            }
                            LoungeEvent::AudioTrackChanged(audio) => {
                                println!("[{}] Audio track changed:", screen_name_for_task);
                                println!("  Audio track ID: {}", audio.audio_track_id);
                                println!("  Video ID: {}", audio.video_id);
                            }
                            LoungeEvent::PlaylistModified(playlist) => {
                                println!("[{}] Playlist modified:", screen_name_for_task);
                                println!("  Current index: {:?}", playlist.current_index);
                                println!("  First video ID: {}", playlist.first_video_id);
                                println!("  List ID: {}", playlist.list_id);
                                println!("  Video ID: {}", playlist.video_id);
                            }
                            LoungeEvent::AutoplayUpNext(next) => {
                                println!("[{}] Autoplay up next:", screen_name_for_task);
                                println!("  Video ID: {}", next.video_id);
                            }
                            LoungeEvent::VolumeChanged(volume) => {
                                println!("[{}] Volume changed:", screen_name_for_task);
                                println!("  Volume level: {}", volume.volume_level());
                                println!("  Muted: {}", volume.is_muted());
                            }
                        }
                    }
                    Err(e) => match e {
                        tokio::sync::broadcast::error::RecvError::Closed => {
                            println!("[{}] Event channel closed", screen_name_for_task);
                            break;
                        }
                        tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
                            println!(
                                "[{}] Warning: Event receiver lagging behind, missed {} events",
                                screen_name_for_task, missed
                            );
                        }
                    },
                }
            }
        });

        // Return the handler
        Ok(ClientHandler {
            client,
            screen_name,
            task_handle: Some(task_handle),
        })
    }

    // Process command line arguments
    let args: Vec<String> = env::args().collect();
    let mut pair_mode = false;
    let mut pairing_code = String::new();
    let mut screen_name = String::new();

    // Check if we're in pairing mode
    if args.len() > 1 {
        if args[1] == "help" {
            println!("YouTube Lounge API Multiple Screens Example");
            println!("Usage:");
            println!("  cargo run --example multiple_screens              - Connect to all paired screens");
            println!("  cargo run --example multiple_screens pair CODE    - Pair with a new screen using the CODE");
            println!("  cargo run --example multiple_screens debug        - Connect with debug mode to see raw event data");
            println!(
                "  cargo run --example multiple_screens pair CODE NAME - Pair with a named screen"
            );
            return Ok(());
        } else if args[1] == "pair" {
            pair_mode = true;

            if args.len() > 2 {
                // Remove any spaces from the pairing code
                pairing_code = args[2].replace(" ", "");
            } else {
                eprintln!(
                    "Usage: cargo run --example multiple_screens pair <pairing_code> [screen_name]"
                );
                return Ok(());
            }
        }
    }

    // Optional screen name for identification
    if args.len() > 3 {
        screen_name = args[3].clone();
    }

    // Check for debug mode flag
    let debug_mode = args.contains(&"debug".to_string());
    if debug_mode {
        println!("Debug mode enabled - will show raw JSON payloads");
    }

    // Load existing authentication data
    let mut auth_store = load_auth_data();

    // If we're in pair mode, add the new screen
    if pair_mode {
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
            stored.name = Some(screen_name);
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
    }

    // Make sure we have screens to connect to
    if auth_store.screens.is_empty() {
        eprintln!("No paired screens found. Use 'cargo run --example multiple_screens pair <code>' to pair.");
        return Ok(());
    }

    println!("Found {} paired screens", auth_store.screens.len());

    // Create a vec to hold all client handlers
    let mut handlers = Vec::new();

    // Connect to all screens
    for (screen_id, stored_screen) in auth_store.screens.iter() {
        println!(
            "Connecting to screen: {} ({})",
            stored_screen.name.as_deref().unwrap_or("Unnamed"),
            screen_id
        );

        // Try to connect to this screen
        match connect_to_screen(&stored_screen, debug_mode, auth_store.clone()).await {
            Ok(handler) => {
                println!("Successfully connected to screen: {}", handler.screen_name);

                // Add to our list of handlers
                handlers.push(handler);
            }
            Err(e) => {
                eprintln!(
                    "Failed to connect to screen {}: {}",
                    stored_screen.name.as_deref().unwrap_or(screen_id),
                    e
                );

                // Continue with other screens
                continue;
            }
        }
    }

    // Check if we have any active connections
    if handlers.is_empty() {
        eprintln!("Failed to connect to any screens. Exiting.");
        return Ok(());
    }

    println!("Successfully connected to {} screens", handlers.len());
    println!("Monitoring all connected screens for events. Press Ctrl+C to exit.");

    // Let all screens run and gather events for a while
    println!("Monitoring events for 20 seconds...");
    sleep(Duration::from_secs(20)).await;

    // Demonstrate getting sessions from multiple clients
    println!("\n=== Multiple Client Session Status ===");
    for handler in &handlers {
        if let Some(session) = handler.client.get_current_session() {
            println!("[{}] Current session:", handler.screen_name);
            println!("  Video: {}", session.video_id.as_deref().unwrap_or("None"));
            println!("  State: {}", session.state_name());
            println!("  Progress: {:.1}%", session.progress_percentage());
        } else {
            println!("[{}] No active session", handler.screen_name);
        }
    }

    // Demonstrate playing a video on all connected screens
    println!("\nPlaying a video on all connected screens...");
    let video_id = "dQw4w9WgXcQ"; // Rick Astley

    // Play the video on each screen
    for handler in &mut handlers {
        let video_id_str = video_id.to_string();
        println!("[{}] Sending play command...", handler.screen_name);
        match handler
            .client
            .send_command_with_refresh(PlaybackCommand::set_playlist(video_id_str))
            .await
        {
            Ok(_) => println!(
                "[{}] Started playback of video: {}",
                handler.screen_name, video_id
            ),
            Err(e) => eprintln!("[{}] Failed to start playback: {}", handler.screen_name, e),
        }
    }

    // Let the videos play for a while to observe events
    println!("\nWaiting for 10 seconds to observe events...");
    sleep(Duration::from_secs(10)).await;

    // Clean up - disconnect all clients
    println!("\nDisconnecting from all screens...");

    // Disconnect each client
    for mut handler in handlers {
        println!("[{}] Disconnecting...", handler.screen_name);

        // Disconnect the client
        match handler.client.disconnect().await {
            Ok(_) => println!("[{}] Disconnected from screen", handler.screen_name),
            Err(e) => eprintln!("[{}] Failed to disconnect: {}", handler.screen_name, e),
        }

        // Abort the task handler
        if let Some(handle) = handler.task_handle.take() {
            handle.abort();
            println!("[{}] Event handler aborted", handler.screen_name);
        }
    }

    println!("All screens disconnected, example complete.");

    Ok(())
}
