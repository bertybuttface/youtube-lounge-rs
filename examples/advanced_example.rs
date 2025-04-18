use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self};
use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use youtube_lounge_rs::{LoungeClient, LoungeEvent, Screen};

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

// Default function for enabled field
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

use fs2::FileExt;
use std::sync::{Arc, Mutex};

// Create a global file lock for auth store updates
lazy_static::lazy_static! {
    static ref AUTH_FILE_LOCK: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
}

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

    // Create clients - either with stored auth or by pairing
    let clients = if pairing_code.is_some() {
        info!(
            "Pairing with new screen using code: {}",
            pairing_code.as_ref().unwrap()
        );
        match create_client_with_pairing(pairing_code.as_ref().unwrap()).await {
            Ok(client) => vec![client],
            Err(e) => {
                error!("Failed to pair with screen: {}", e);
                info!(
                    "Make sure the pairing code is correct and your TV is on the pairing screen."
                );
                return Ok(());
            }
        }
    } else {
        match create_clients_from_stored_auth().await {
            Ok(clients) => {
                info!(
                    "Using stored authentication data for {} screens",
                    clients.len()
                );
                clients
            }
            Err(e) => {
                error!("Failed to initialize clients with stored auth: {}", e);
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

    // Store event handlers to keep them alive
    let mut event_handlers = Vec::new();
    let mut connected_clients = Vec::new();

    // Process each client
    for client in clients {
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
                warn!("[{}] Screen is not available, cannot connect. The screen may be offline or unreachable.", screen_id);
                warn!(
                    "[{}] Check that the YouTube app is open on your TV/device.",
                    screen_id
                );
                continue; // Skip this client and try the next one
            }
            Err(e) => {
                warn!("[{}] Failed to check screen availability: {}", screen_id, e);
                warn!(
                    "[{}] Check your network connection and try again.",
                    screen_id
                );
                continue; // Skip this client and try the next one
            }
        }

        // Step 5: Connect to the screen
        info!("[{}] Connecting to screen", screen_id);
        match client.connect_with_refresh().await {
            Ok(_) => info!("[{}] Successfully connected to screen", screen_id),
            Err(e) => {
                warn!("[{}] Failed to connect to screen: {}", screen_id, e);
                warn!(
                    "[{}] Check that the screen is still available and the YouTube app is open.",
                    screen_id
                );
                continue; // Skip this client and try the next one
            }
        }

        // Step 6: Handle events in a separate task
        info!("[{}] Starting event handling", screen_id);

        // Create a separate async task to receive events
        let screen_id_clone = screen_id.clone();
        let event_handler = tokio::spawn(async move {
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

        // Store the event handler and connected client
        event_handlers.push(event_handler);
        connected_clients.push(client);
    }

    // Check if we have any connected clients
    if connected_clients.is_empty() {
        error!("Failed to connect to any screens.");
        return Ok(());
    }

    info!(
        "Successfully connected to {} screens",
        connected_clients.len()
    );

    // Wait a moment for connections to stabilize
    sleep(Duration::from_secs(1)).await;

    // Print info message about listening for events
    info!("Listening for events from all screens. Press Ctrl+C to exit.");

    // Wait for Ctrl+C signal which works on all platforms
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("Received Ctrl+C, shutting down...");
        }
        Err(err) => {
            error!("Failed to listen for Ctrl+C signal: {}", err);
        }
    }

    // Step 8: Disconnect all clients
    for client in &mut connected_clients {
        let screen_id = client.screen_id().to_string();
        info!("[{}] Disconnecting...", screen_id);
        if let Err(e) = client.disconnect().await {
            error!("[{}] Error during disconnect: {}", screen_id, e);
        }
    }

    // Give some time for last events to process
    sleep(Duration::from_secs(1)).await;

    // We don't actually need to wait for the event handlers to complete
    // as they will terminate when the program exits

    Ok::<(), Box<dyn Error + Send + Sync>>(())
}

fn update_token_in_auth_store(
    screen_id: &str,
    new_token: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    info!("[{}] Updating stored lounge token", screen_id);

    // Acquire the mutex lock to ensure only one thread tries to update at a time
    let _guard = match AUTH_FILE_LOCK.lock() {
        Ok(guard) => guard,
        Err(e) => {
            error!(
                "[{}] Failed to acquire lock for auth file: {}",
                screen_id, e
            );
            return Err(Box::new(io::Error::new(
                io::ErrorKind::Other,
                format!("Lock acquisition failed: {}", e),
            )));
        }
    };

    debug!("[{}] Acquired auth file lock for token update", screen_id);

    // Open the auth file with explicit file locking
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(AUTH_FILENAME)
    {
        Ok(file) => file,
        Err(e) => {
            error!("[{}] Failed to open auth file for update: {}", screen_id, e);
            return Err(Box::new(e));
        }
    };

    // Acquire an exclusive lock on the file
    if let Err(e) = file.lock_exclusive() {
        error!("[{}] Failed to acquire file lock: {}", screen_id, e);
        return Err(Box::new(e));
    }

    debug!("[{}] Acquired exclusive file lock", screen_id);

    // Read the current content
    let mut contents = String::new();
    let mut auth_store: AuthStore =
        match std::io::Read::read_to_string(&mut file.try_clone()?, &mut contents) {
            Ok(_) => {
                // Parse the file content
                match serde_json::from_str(&contents) {
                    Ok(store) => store,
                    Err(e) => {
                        error!("[{}] Failed to parse auth file: {}", screen_id, e);

                        // Release the lock before returning
                        let _ = FileExt::unlock(&file);
                        return Err(Box::new(e));
                    }
                }
            }
            Err(e) => {
                error!("[{}] Failed to read auth file: {}", screen_id, e);

                // Release the lock before returning
                let _ = FileExt::unlock(&file);
                return Err(Box::new(e));
            }
        };

    // Update the token if the screen exists in the store
    if let Some(screen) = auth_store.screens.get_mut(screen_id) {
        debug!(
            "[{}] Updating token from {} to {}",
            screen_id, screen.lounge_token, new_token
        );
        screen.lounge_token = new_token.to_string();

        // Generate updated JSON
        let json = match serde_json::to_string_pretty(&auth_store) {
            Ok(j) => j,
            Err(e) => {
                error!("[{}] Failed to serialize auth data: {}", screen_id, e);

                // Release the lock before returning
                let _ = FileExt::unlock(&file);
                return Err(Box::new(io::Error::new(io::ErrorKind::InvalidData, e)));
            }
        };

        // Truncate the file and write new content
        if let Err(e) = file.set_len(0) {
            error!("[{}] Failed to truncate auth file: {}", screen_id, e);

            // Release the lock before returning
            let _ = FileExt::unlock(&file);
            return Err(Box::new(e));
        }

        // Seek to the beginning
        use std::io::Seek;
        if let Err(e) = file.seek(std::io::SeekFrom::Start(0)) {
            error!(
                "[{}] Failed to seek to beginning of auth file: {}",
                screen_id, e
            );

            // Release the lock before returning
            let _ = FileExt::unlock(&file);
            return Err(Box::new(e));
        }

        // Write the updated content
        if let Err(e) =
            std::io::Write::write_all(&mut std::io::BufWriter::new(&file), json.as_bytes())
        {
            error!("[{}] Failed to write updated auth data: {}", screen_id, e);

            // Release the lock before returning
            let _ = FileExt::unlock(&file);
            return Err(Box::new(e));
        }

        // Explicitly release the lock
        if let Err(e) = fs2::FileExt::unlock(&file) {
            warn!("[{}] Failed to release file lock: {}", screen_id, e);
            // Continue since the file will be unlocked when it's closed anyway
        }

        info!(
            "[{}] Successfully updated lounge token in auth store",
            screen_id
        );
        Ok(())
    } else {
        // Release the lock before returning
        let _ = FileExt::unlock(&file);

        let err = io::Error::new(
            io::ErrorKind::NotFound,
            format!("Screen {} not found in auth store", screen_id),
        );
        error!("[{}] Screen not found in auth store", screen_id);
        Err(Box::new(err))
    }
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

    // Set the token refresh callback
    client
        .set_token_refresh_callback(move |screen_id, new_token| {
            if let Err(e) = update_token_in_auth_store(screen_id, new_token) {
                error!("[{}] Error updating token in auth store: {}", screen_id, e);
            }
        })
        .await;

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

// Create clients from stored auth
async fn create_clients_from_stored_auth() -> Result<Vec<LoungeClient>, Box<dyn Error + Send + Sync>>
{
    // Load auth data from file
    let auth_store = match load_auth() {
        Ok(store) => store,
        Err(e) => {
            error!("Failed to load authentication data: {}", e);
            return Err(Box::new(e));
        }
    };

    // Get stored screen info for all enabled screens
    if auth_store.screens.is_empty() {
        let err = io::Error::new(
            io::ErrorKind::InvalidData,
            "Auth file exists but contains no screens",
        );
        error!("No screens found in auth file");
        return Err(Box::new(err));
    }

    let mut clients = Vec::new();

    // Create clients for all enabled screens
    for (screen_id, stored_screen) in auth_store.screens.iter() {
        // Skip disabled screens
        if !stored_screen.enabled {
            info!(
                "Skipping disabled screen: {} (ID: {})",
                stored_screen.name.as_deref().unwrap_or("Unknown"),
                screen_id
            );
            continue;
        }

        // Check if screen data looks valid
        if stored_screen.screen_id.trim().is_empty() {
            warn!(
                "Invalid screen data in auth file: empty screen_id for {}, skipping",
                screen_id
            );
            continue;
        }

        if stored_screen.lounge_token.trim().is_empty() {
            warn!(
                "Invalid screen data in auth file: empty lounge_token for {}, skipping",
                screen_id
            );
            continue;
        }

        if stored_screen.device_id.trim().is_empty() {
            warn!(
                "Invalid screen data in auth file: empty device_id for {}, skipping",
                screen_id
            );
            continue;
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

        // Set the token refresh callback
        client
            .set_token_refresh_callback(move |screen_id, new_token| {
                if let Err(e) = update_token_in_auth_store(screen_id, new_token) {
                    error!("[{}] Error updating token in auth store: {}", screen_id, e);
                }
            })
            .await;

        info!(
            "Created client for screen: {} (ID: {})",
            stored_screen.name.as_deref().unwrap_or("Unknown"),
            stored_screen.screen_id
        );
        debug!("Using lounge token: {}", stored_screen.lounge_token);
        debug!("Using device ID: {}", stored_screen.device_id);

        clients.push(client);
    }

    if clients.is_empty() {
        let err = io::Error::new(
            io::ErrorKind::InvalidData,
            "No valid enabled screens found in auth file",
        );
        error!("No valid enabled screens found in auth file");
        return Err(Box::new(err));
    }

    info!("Created {} clients for enabled screens", clients.len());
    Ok(clients)
}

// Save auth data to file
fn save_auth(auth: &AuthStore) -> io::Result<()> {
    // Acquire the mutex lock to ensure only one thread tries to update at a time
    let _guard = match AUTH_FILE_LOCK.lock() {
        Ok(guard) => guard,
        Err(e) => {
            error!("Failed to acquire lock for auth file: {}", e);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Lock acquisition failed: {}", e),
            ));
        }
    };

    debug!("Acquired auth file lock for full save");

    // Generate JSON
    let json = match serde_json::to_string_pretty(auth) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize auth data: {}", e);
            return Err(io::Error::new(io::ErrorKind::InvalidData, e));
        }
    };

    debug!("Serialized auth data to JSON, size: {} bytes", json.len());

    // Open file with explicit locking
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(AUTH_FILENAME)
    {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to open auth file '{}': {}", AUTH_FILENAME, e);
            return Err(e);
        }
    };

    // Acquire an exclusive lock on the file
    if let Err(e) = file.lock_exclusive() {
        error!("Failed to acquire file lock: {}", e);
        return Err(e);
    }

    debug!("Acquired exclusive file lock for full save");

    // Write data
    let result = std::io::Write::write_all(&mut std::io::BufWriter::new(&file), json.as_bytes())
        .map_err(|e| {
            error!(
                "Failed to write data to auth file '{}': {}",
                AUTH_FILENAME, e
            );
            e
        });

    // Explicitly release the lock
    if let Err(e) = fs2::FileExt::unlock(&file) {
        warn!("Failed to release file lock: {}", e);
        // Continue since the file will be unlocked when it's closed anyway
    }

    match result {
        Ok(_) => {
            info!("Successfully saved auth data to '{}'", AUTH_FILENAME);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

// Load auth data from file
fn load_auth() -> io::Result<AuthStore> {
    // Acquire the mutex lock to ensure only one thread tries to read at a time
    let _guard = match AUTH_FILE_LOCK.lock() {
        Ok(guard) => guard,
        Err(e) => {
            error!("Failed to acquire lock for auth file: {}", e);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Lock acquisition failed: {}", e),
            ));
        }
    };

    debug!("Acquired auth file lock for loading");

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

    // Open file with shared lock (for reading)
    let file = match std::fs::OpenOptions::new().read(true).open(AUTH_FILENAME) {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to open auth file '{}': {}", AUTH_FILENAME, e);
            return Err(e);
        }
    };

    // Acquire a shared lock on the file
    if let Err(e) = fs2::FileExt::lock_shared(&file) {
        error!("Failed to acquire shared file lock: {}", e);
        return Err(e);
    }

    debug!("Acquired shared file lock for reading");

    // Read the file content
    let mut contents = String::new();
    let read_result =
        std::io::Read::read_to_string(&mut std::io::BufReader::new(&file), &mut contents).map_err(
            |e| {
                error!("Failed to read auth file '{}': {}", AUTH_FILENAME, e);
                e
            },
        );

    // Parse JSON if read was successful
    let parse_result = match read_result {
        Ok(bytes) => {
            debug!("Read {} bytes from auth file", bytes);

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
        Err(e) => Err(e),
    };

    // Explicitly release the lock
    if let Err(e) = fs2::FileExt::unlock(&file) {
        warn!("Failed to release file lock: {}", e);
        // Continue since the file will be unlocked when it's closed anyway
    }

    parse_result
}
