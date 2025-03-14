use std::error::Error;
use tokio::time::{sleep, Duration};
use youtube_lounge_rs::{HasPlaybackState, LoungeClient, LoungeEvent, PlaybackCommand};

/// A basic example showing how to pair with a screen, connect, and control playback
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Step 1: Pair with a screen using the pairing code shown on your TV
    // This only needs to be done once per device - you can store the credentials
    let pairing_code = std::env::args()
        .nth(1)
        .expect("Please provide a pairing code");
    let screen = LoungeClient::pair_with_screen(&pairing_code).await?;

    println!("Successfully paired with screen: {:?}", screen);

    // Step 2: Create client using the screen information from pairing
    // For a persistent device_id, you could load a saved device_id from a config file
    // and use LoungeClient::with_device_id instead
    let mut client = LoungeClient::new(
        &screen.screen_id,
        &screen.lounge_token,
        "Rust YouTube Controller",
    );

    // Get the device_id that was generated - in a real app, you might want to save this
    // for future sessions to maintain continuity
    println!("Using device ID: {}", client.device_id());

    // Enable debug mode to see all raw event data
    client.enable_debug_mode();
    println!("Debug mode enabled - will show raw JSON payloads for events");

    // Step 3: Subscribe to events before connecting
    let mut receiver = client.event_receiver();

    // Optional: Set a callback for token refreshes if you want to persist the refreshed token
    client.set_token_refresh_callback(|screen_id, new_token| {
        println!("Token refreshed for screen {}: {}", screen_id, new_token);
        // In a real app, you would save this new token for future use
    });

    // Step 4: Check if the screen is available (with automatic token refresh if needed)
    let available = client.check_screen_availability_with_refresh().await;
    println!("Screen available: {:?}", available);

    // Step 5: Connect to the device - for simplicity, let's use check_screen_availability_with_refresh
    // and then connect, since connecting doesn't have its own automatic refresh method
    println!("Connecting to screen...");

    // Create another receiver for session updates
    let mut session_rx = client.session_receiver();

    // Make sure screen is available and token is valid before connecting
    if client.check_screen_availability_with_refresh().await? {
        // Connect to the screen with valid token
        client.connect().await?;
        println!("Connected to screen");
    } else {
        println!("Screen is not available, cannot connect");
        return Ok(());
    }

    // Step 6: Spawn tasks to handle events and session updates from the TV

    // Task to handle session updates
    tokio::spawn(async move {
        loop {
            match session_rx.recv().await {
                Ok(session) => {
                    println!("Session update:");
                    if let Some(video_id) = &session.video_id {
                        println!("  Video ID: {}", video_id);
                    }
                    // The title field is not directly available in PlaybackSession
                    println!("  Status: {:?}", session.status());
                    println!(
                        "  Position: {:.1}s / {:.1}s",
                        session.current_time, session.duration
                    );
                }
                Err(e) => match e {
                    tokio::sync::broadcast::error::RecvError::Closed => {
                        println!("Session channel closed");
                        break;
                    }
                    tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
                        println!("Missed {} session updates due to lagging", missed);
                    }
                },
            }
        }
    });

    // Task to handle event updates
    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => match event {
                    LoungeEvent::NowPlaying(np) => {
                        println!("Now playing: {}", np.video_data.title);
                    }
                    LoungeEvent::StateChange(state) => {
                        println!("State changed: {}", state.state_name());
                        println!(
                            "  Video ID: {} - Duration: {:.1}s",
                            state.video_id, state.duration
                        );
                        println!(
                            "  Current time: {:.1}s - State: {}",
                            state.current_time, state.state
                        );
                        println!("  Volume: {} - Muted: {}", state.volume, state.muted);
                    }
                    LoungeEvent::ScreenDisconnected => {
                        println!("Screen disconnected");
                        break;
                    }
                    LoungeEvent::SessionEstablished => {
                        println!("Session established - ready to send commands");
                    }
                    LoungeEvent::Unknown(event_info) => {
                        println!("======= UNKNOWN EVENT =======");
                        println!("{}", event_info);
                        println!("=============================");
                    }
                    _ => println!("Other event: {:?}", event),
                },
                Err(e) => match e {
                    tokio::sync::broadcast::error::RecvError::Closed => {
                        println!("Event channel closed");
                        break;
                    }
                    tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
                        println!("Missed {} events due to lagging", missed);
                    }
                },
            }
        }
    });

    // Step 7: Wait a moment for the connection to stabilize
    sleep(Duration::from_secs(1)).await;

    // Step 8: Send commands to control playback

    // Play a specific video
    println!("Starting a video...");
    client
        .send_command_with_refresh(
            PlaybackCommand::set_playlist("dQw4w9WgXcQ".to_string()), // Rick Astley
        )
        .await?;

    // Wait for the video to start
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

    // Seek to a specific position (in seconds)
    println!("Seeking to 30 seconds...");
    client
        .send_command_with_refresh(PlaybackCommand::SeekTo { new_time: 30.0 })
        .await?;

    sleep(Duration::from_secs(3)).await;

    // Adjust volume
    println!("Setting volume to 50%...");
    client
        .send_command_with_refresh(PlaybackCommand::SetVolume { volume: 50 })
        .await?;

    // Wait a bit more to observe the results and see if we receive events
    // from actions performed directly in the YouTube client
    println!("\nNow waiting for 60 seconds - please perform actions in the YouTube client");
    println!("Try playing, pausing, seeking, etc. directly on your device");
    println!("Watch for onStateChange events in the debug output\n");
    sleep(Duration::from_secs(60)).await;

    // Step 9: Disconnect from the screen when done
    println!("Disconnecting...");
    client.disconnect().await?;

    Ok(())
}
