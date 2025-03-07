use std::error::Error;
use tokio::time::{sleep, Duration};
use youtube_lounge_rs::{LoungeClient, LoungeEvent, PlaybackCommand};

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
    let mut client = LoungeClient::new(
        &screen.screen_id,
        &screen.lounge_token,
        "Rust YouTube Controller",
    );

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

    // Make sure screen is available and token is valid before connecting
    if client.check_screen_availability_with_refresh().await? {
        // Connect to the screen with valid token
        client.connect().await?;
        println!("Connected to screen");
    } else {
        println!("Screen is not available, cannot connect");
        return Ok(());
    }

    // Step 6: Spawn a task to handle events from the TV
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            match event {
                LoungeEvent::NowPlaying(np) => {
                    println!("Now playing: {}", np.video_data.title);
                }
                LoungeEvent::StateChange(state) => {
                    println!("State changed: {}", state.state_name());
                }
                _ => println!("Received event: {:?}", event),
            }
        }
    });

    // Step 7: Wait a moment for the connection to stabilize
    sleep(Duration::from_secs(1)).await;

    // Step 8: Send commands to control playback

    // Play a specific video
    println!("Starting a video...");
    client
        .send_command_with_refresh(PlaybackCommand::SetPlaylist {
            video_id: "dQw4w9WgXcQ".to_string(), // Rick Astley - Never Gonna Give You Up
        })
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

    // Wait a bit more to observe the results
    sleep(Duration::from_secs(5)).await;

    // Step 9: Disconnect from the screen when done
    println!("Disconnecting...");
    client.disconnect().await?;

    Ok(())
}
