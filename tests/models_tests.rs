use serde_json::json;

use youtube_lounge_rs::{Device, DeviceInfo, HasVolume, NowPlaying, PlaybackState, Screen};

// Test Screen model serialization/deserialization
#[test]
fn test_screen_model() {
    // Test deserialization
    let json_data = json!({
        "screenId": "test_screen_id",
        "loungeToken": "test_lounge_token",
        "name": "Test Screen"
    });

    let screen: Screen = serde_json::from_value(json_data).unwrap();

    assert_eq!(screen.screen_id, "test_screen_id");
    assert_eq!(screen.lounge_token, "test_lounge_token");
    assert_eq!(screen.name, Some("Test Screen".to_string()));
}

// Test PlaybackState model serialization/deserialization
#[test]
fn test_playback_state_model() {
    // Test deserialization
    let json_data = json!({
        "state": "1",
        "currentTime": "42.5",
        "duration": "180.0",
        "seekableStartTime": "0.0",
        "seekableEndTime": "180.0",
        "videoId": "dQw4w9WgXcQ",
        "volume": "50",
        "muted": "false",
        "loadedTime": "60.0",
        "cpn": "test_cpn",
        "videoData": {
            "video_id": "dQw4w9WgXcQ",
            "author": "Rick Astley",
            "title": "Never Gonna Give You Up",
            "is_playable": true
        }
    });

    let state: PlaybackState = serde_json::from_value(json_data).unwrap();

    assert_eq!(state.state, "1");
    assert_eq!(state.current_time, "42.5");
    assert_eq!(state.duration, "180.0");
    assert_eq!(state.seekable_start_time, "0.0");
    assert_eq!(state.seekable_end_time, "180.0");
    assert_eq!(state.video_id, "dQw4w9WgXcQ");
    assert_eq!(state.volume, "50");
    assert_eq!(state.muted, "false");
    assert_eq!(state.loaded_time, "60.0");
    assert_eq!(state.cpn, Some("test_cpn".to_string()));

    // Test the value parsing methods
    assert_eq!(state.state_value(), 1);
    assert_eq!(state.current_time_value(), 42.5);
    assert_eq!(state.duration_value(), 180.0);
    assert_eq!(state.seekable_start_time_value(), 0.0);
    assert_eq!(state.seekable_end_time_value(), 180.0);
    assert_eq!(state.volume_value(), 50);
    assert!(!state.is_muted());
    assert_eq!(state.loaded_time_value(), 60.0);
}

// Test NowPlaying model serialization/deserialization
#[test]
fn test_now_playing_model() {
    // Test deserialization
    let json_data = json!({
        "videoId": "dQw4w9WgXcQ",
        "currentTime": "42.5",
        "state": "1",
        "duration": "180.0",
        "loadedTime": "60.0",
        "seekableStartTime": "0.0",
        "seekableEndTime": "180.0",
        "listId": "PL12345",
        "cpn": "test_cpn",
        "mdxExpandedReceiverVideoIdList": "abc123,def456",
        "videoData": {
            "video_id": "dQw4w9WgXcQ",
            "author": "Rick Astley",
            "title": "Never Gonna Give You Up",
            "is_playable": true
        }
    });

    let now_playing: NowPlaying = serde_json::from_value(json_data).unwrap();

    assert_eq!(now_playing.video_id, "dQw4w9WgXcQ");
    assert_eq!(now_playing.current_time, "42.5");
    assert_eq!(now_playing.state, "1");
    assert_eq!(now_playing.duration, "180.0");
    assert_eq!(now_playing.loaded_time, "60.0");
    assert_eq!(now_playing.seekable_start_time, "0.0");
    assert_eq!(now_playing.seekable_end_time, "180.0");
    assert_eq!(now_playing.list_id, Some("PL12345".to_string()));
    assert_eq!(now_playing.cpn, Some("test_cpn".to_string()));
    assert_eq!(
        now_playing.mdx_expanded_receiver_video_id_list,
        Some("abc123,def456".to_string())
    );

    // Test the value parsing methods
    assert_eq!(now_playing.state_value(), 1);
    assert_eq!(now_playing.current_time_value(), 42.5);
    assert_eq!(now_playing.duration_value(), 180.0);
    assert_eq!(now_playing.seekable_start_time_value(), 0.0);
    assert_eq!(now_playing.seekable_end_time_value(), 180.0);
    assert_eq!(now_playing.loaded_time_value(), 60.0);

    // Test the video history parsing
    let video_history = now_playing.video_history().unwrap();
    assert_eq!(
        video_history,
        vec!["abc123".to_string(), "def456".to_string()]
    );
}

// Test Device model serialization/deserialization
#[test]
fn test_device_model() {
    // Test deserialization
    let device_info_json = json!({
        "brand": "Roku",
        "model": "Ultra",
        "deviceType": "TV"
    })
    .to_string();

    let json_data = json!({
        "app": "YouTube",
        "name": "Living Room TV",
        "id": "device123",
        "type": "SMART_TV",
        "deviceInfo": device_info_json
    });

    let device: Device = serde_json::from_value(json_data).unwrap();

    assert_eq!(device.app, "YouTube");
    assert_eq!(device.name, "Living Room TV");
    assert_eq!(device.id, "device123");
    assert_eq!(device.device_type, "SMART_TV");
    assert_eq!(device.device_info_raw, device_info_json);

    // Check device_info (should be None initially until explicitly parsed)
    assert!(device.device_info.is_none());
}

// Test DeviceInfo model serialization/deserialization
#[test]
fn test_device_info_model() {
    // Test deserialization
    let json_data = json!({
        "brand": "Samsung",
        "model": "Smart TV",
        "deviceType": "TV"
    });

    let device_info: DeviceInfo = serde_json::from_value(json_data).unwrap();

    assert_eq!(device_info.brand, "Samsung");
    assert_eq!(device_info.model, "Smart TV");
    assert_eq!(device_info.device_type, "TV");
}
