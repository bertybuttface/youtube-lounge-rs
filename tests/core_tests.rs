use serde_json::json;
use youtube_lounge_rs::{
    AdState, Device, DeviceInfo, LoungeClient, LoungeError, LoungeEvent, NowPlaying,
    PlaybackCommand, PlaybackSession, PlaybackState, Screen, YoutubeValueParser,
};

// Test model serialization and deserialization
#[test]
fn test_models() {
    // Test Screen deserialization
    let screen_json = json!({
        "screenId": "test_screen_id",
        "loungeToken": "test_lounge_token",
        "name": "Test Screen"
    });
    let screen: Screen = serde_json::from_value(screen_json).unwrap();
    assert_eq!(screen.screen_id, "test_screen_id");
    assert_eq!(screen.lounge_token, "test_lounge_token");
    assert_eq!(screen.name, Some("Test Screen".to_string()));

    // Test PlaybackState deserialization
    let state_json = json!({
        "state": "1",
        "currentTime": "42.5",
        "duration": "180.0",
        "loadedTime": "60.0",
        "cpn": "test_cpn"
    });
    let state: PlaybackState = serde_json::from_value(state_json).unwrap();
    assert_eq!(state.state, "1");
    assert_eq!(state.current_time, "42.5");
    assert_eq!(state.loaded_time, "60.0");
    assert_eq!(state.cpn, Some("test_cpn".to_string()));

    // Test Device deserialization
    let device_json = json!({
        "app": "YouTube",
        "name": "Living Room TV",
        "id": "device123",
        "type": "SMART_TV",
        "deviceInfo": "{\"brand\":\"LG\",\"model\":\"OLED65\",\"deviceType\":\"TV\"}"
    });
    let device: Device = serde_json::from_value(device_json).unwrap();
    assert_eq!(device.app, "YouTube");
    assert_eq!(device.name, "Living Room TV");
    assert_eq!(device.id, "device123");
    assert_eq!(device.device_type, "SMART_TV");
    assert_eq!(
        device.device_info_raw,
        "{\"brand\":\"LG\",\"model\":\"OLED65\",\"deviceType\":\"TV\"}"
    );

    // Test DeviceInfo parsing from raw string
    let device_info_json = r#"{"brand":"LG","model":"OLED65","deviceType":"TV"}"#;
    let device_info: DeviceInfo = serde_json::from_str(device_info_json).unwrap();
    assert_eq!(device_info.brand, "LG");
    assert_eq!(device_info.model, "OLED65");
    assert_eq!(device_info.device_type, "TV");
}

// Test the event variants
#[test]
fn test_events() {
    // Test creating a StateChange event
    let playback_state = PlaybackState {
        state: "1".to_string(),
        current_time: "42.5".to_string(),
        duration: "180.0".to_string(),
        cpn: Some("test_cpn".to_string()),
        loaded_time: "60.0".to_string(),
    };
    let event = LoungeEvent::StateChange(playback_state);

    match event {
        LoungeEvent::StateChange(state) => {
            assert_eq!(state.current_time, "42.5");
            assert_eq!(state.state, "1");
            assert_eq!(state.duration, "180.0");
        }
        _ => panic!("Expected StateChange event"),
    }

    // Test NowPlaying event
    let now_playing = NowPlaying {
        video_id: "dQw4w9WgXcQ".to_string(),
        current_time: "42.5".to_string(),
        state: "1".to_string(),
        video_data: None,
        cpn: Some("test_cpn".to_string()),
        list_id: Some("PLtestlist".to_string()),
    };
    let event = LoungeEvent::NowPlaying(now_playing);

    match event {
        LoungeEvent::NowPlaying(np) => {
            assert_eq!(np.video_id, "dQw4w9WgXcQ");
            assert_eq!(np.current_time, "42.5");
        }
        _ => panic!("Expected NowPlaying event"),
    }

    // Test AdStateChange event
    let ad_state = AdState {
        content_video_id: "adVideoId123".to_string(),
        is_skip_enabled: true,
    };
    let event = LoungeEvent::AdStateChange(ad_state);
    match event {
        LoungeEvent::AdStateChange(ad) => {
            assert_eq!(ad.content_video_id, "adVideoId123");
            assert!(ad.is_skip_enabled);
            assert!(ad.is_skippable()); // Test helper method
        }
        _ => panic!("Expected AdStateChange event"),
    }
}

// Test YoutubeValueParser utility trait
#[test]
fn test_youtube_value_parser() {
    // Test parse_float
    assert_eq!(<str as YoutubeValueParser>::parse_float("42.5"), 42.5);
    assert_eq!(
        <str as YoutubeValueParser>::parse_float("not_a_number"),
        0.0
    ); // Default value

    // Test parse_int
    assert_eq!(<str as YoutubeValueParser>::parse_int("42"), 42);
    assert_eq!(<str as YoutubeValueParser>::parse_int("not_a_number"), 0); // Default value

    // Test parse_bool
    assert!(<str as YoutubeValueParser>::parse_bool("true"));
    assert!(!<str as YoutubeValueParser>::parse_bool("false"));
    assert!(!<str as YoutubeValueParser>::parse_bool("anything_else"));

    // Test parse_list
    let list = <str as YoutubeValueParser>::parse_list("item1,item2,item3");
    assert_eq!(list, vec!["item1", "item2", "item3"]);

    // Test PlaybackSession event
    let state = PlaybackState {
        state: "1".to_string(),
        current_time: "42.5".to_string(),
        duration: "180.0".to_string(),
        cpn: Some("test_cpn".to_string()),
        loaded_time: "60.0".to_string(),
    };

    let np = NowPlaying {
        video_id: "dQw4w9WgXcQ".to_string(),
        current_time: "42.5".to_string(),
        state: "1".to_string(),
        video_data: None,
        cpn: Some("test_cpn".to_string()),
        list_id: Some("PLtestlist".to_string()),
    };

    let session = PlaybackSession::new(&np, &state);
    let event = LoungeEvent::PlaybackSession(session);

    match event {
        LoungeEvent::PlaybackSession(session) => {
            assert_eq!(session.video_id, "dQw4w9WgXcQ");
            assert_eq!(session.current_time, 42.5);
            assert_eq!(session.duration, 180.0);
            assert_eq!(session.state, "1");
            assert_eq!(session.loaded_time, 60.0);
            assert_eq!(session.list_id, Some("PLtestlist".to_string()));
            assert_eq!(session.cpn, Some("test_cpn".to_string()));
        }
        _ => panic!("Expected PlaybackSession event"),
    }
}

// Test client constructors
#[tokio::test]
async fn test_client_constructors() {
    // Test new client with auto-generated device ID
    let client = LoungeClient::new("test_screen_id", "test_token", "Test Device");
    let device_id = client.device_id();
    assert!(!device_id.is_empty());

    // Test client with explicit device ID
    let test_device_id = "persistent_device_id_123";
    let client = LoungeClient::with_device_id(
        "test_screen_id",
        "test_token",
        "Test Device",
        test_device_id,
    );
    assert_eq!(client.device_id(), test_device_id);

    // Test event channel is created by subscribing to it
    let _receiver = client.event_receiver();
}

// Test command builders (without using private methods)
#[test]
fn test_playback_commands() {
    // Test setPlaylist with video ID
    let video_id = "dQw4w9WgXcQ";
    let set_playlist = PlaybackCommand::set_playlist(video_id.to_string());
    match set_playlist {
        PlaybackCommand::SetPlaylist {
            video_id: vid,
            current_index,
            list_id,
            ..
        } => {
            assert_eq!(vid, video_id);
            assert_eq!(current_index, Some(-1));
            assert_eq!(list_id, None);
        }
        _ => panic!("Wrong command type returned"),
    }

    // Test setPlaylist with playlist ID
    let playlist_id = "PL12345";
    let set_playlist = PlaybackCommand::set_playlist_by_id(playlist_id.to_string());
    match set_playlist {
        PlaybackCommand::SetPlaylist {
            video_id,
            current_index,
            list_id,
            ..
        } => {
            assert_eq!(video_id, ""); // Empty for playlist ID
            assert_eq!(current_index, Some(0)); // Start from beginning
            assert_eq!(list_id, Some(playlist_id.to_string()));
        }
        _ => panic!("Wrong command type returned"),
    }

    // Test add_video
    let add_video = PlaybackCommand::add_video(video_id.to_string());
    match add_video {
        PlaybackCommand::AddVideo { video_id: vid, .. } => {
            assert_eq!(vid, video_id);
        }
        _ => panic!("Wrong command type returned"),
    }
}

// Test LoungeError
#[test]
fn test_lounge_error() {
    // Test TokenExpired error
    let err = LoungeError::TokenExpired;
    let error_message = format!("{}", err);
    println!("Error message: {}", error_message);
    assert!(error_message.contains("Token expired"));

    // Test SessionExpired error
    let err = LoungeError::SessionExpired;
    let error_message = format!("{}", err);
    assert!(error_message.contains("Session expired"));

    // Test ConnectionClosed error
    let err = LoungeError::ConnectionClosed;
    let error_message = format!("{}", err);
    assert!(error_message.contains("Connection closed"));

    // Test InvalidResponse error
    let err = LoungeError::InvalidResponse("Test error".to_string());
    let error_message = format!("{}", err);
    assert!(error_message.contains("Test error"));
}

// Test thumbnail URL generation
#[test]
fn test_thumbnail_url() {
    let video_id = "dQw4w9WgXcQ";
    let url = LoungeClient::get_thumbnail_url(video_id, 0);
    assert_eq!(url, "https://img.youtube.com/vi/dQw4w9WgXcQ/0.jpg");
}

// Mock EventEmitter for testing event broadcasting
struct EventEmitter {
    sender: tokio::sync::broadcast::Sender<LoungeEvent>,
}

impl EventEmitter {
    fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(100);
        EventEmitter { sender: tx }
    }

    fn emit(&self, event: LoungeEvent) {
        let _ = self.sender.send(event);
    }
}

// Test event receiver
#[tokio::test]
async fn test_event_receiver() {
    let emitter = EventEmitter::new();
    let mut receiver = emitter.sender.subscribe();

    // Emit test event
    emitter.emit(LoungeEvent::SessionEstablished);

    // Verify event was received
    match receiver.recv().await {
        Ok(event) => {
            match event {
                LoungeEvent::SessionEstablished => {
                    // Test passed
                }
                _ => panic!("Received wrong event type"),
            }
        }
        Err(_) => panic!("Failed to receive event"),
    }
}
