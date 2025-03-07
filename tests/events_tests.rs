use youtube_lounge_rs::models::{AdState, SubtitlesTrackChanged, VideoData};
use youtube_lounge_rs::{Device, LoungeEvent, NowPlaying, PlaybackState};

// Test that we can create different LoungeEvent types
#[test]
fn test_lounge_event_variants() {
    // Test SessionEstablished
    let _event = LoungeEvent::SessionEstablished;

    // Test StateChange
    let video_data = VideoData {
        video_id: "dQw4w9WgXcQ".to_string(),
        author: "Rick Astley".to_string(),
        title: "Never Gonna Give You Up".to_string(),
        is_playable: true,
    };

    let playback_state = PlaybackState {
        state: 1,
        current_time: 42.5,
        duration: 180.0,
        seekable_start_time: 0.0,
        seekable_end_time: 180.0,
        video_id: "dQw4w9WgXcQ".to_string(),
        volume: 50,
        muted: false,
        video_data: video_data.clone(),
    };

    let _event = LoungeEvent::StateChange(playback_state);

    // Test NowPlaying
    let now_playing = NowPlaying {
        video_id: "dQw4w9WgXcQ".to_string(),
        current_time: 42.5,
        list_id: Some("PL12345".to_string()),
        state: 1,
        video_data,
    };

    let _event = LoungeEvent::NowPlaying(now_playing);

    // Test LoungeStatus
    let devices = vec![
        Device {
            app: "YouTube".to_string(),
            name: "Device 1".to_string(),
            device_type: "SMART_TV".to_string(),
            device_info_raw: "{}".to_string(),
            device_info: None,
        },
        Device {
            app: "YouTube".to_string(),
            name: "Device 2".to_string(),
            device_type: "SMART_TV".to_string(),
            device_info_raw: "{}".to_string(),
            device_info: None,
        },
    ];

    let _event = LoungeEvent::LoungeStatus(devices);

    // Test ScreenDisconnected
    let _event = LoungeEvent::ScreenDisconnected;

    // Test Unknown
    let _event = LoungeEvent::Unknown("TestEvent".to_string());

    // Test AdStateChange
    let ad_state = AdState {
        content_video_id: "adVideoId123".to_string(),
        is_skip_enabled: true,
    };

    let _event = LoungeEvent::AdStateChange(ad_state);

    // Test SubtitlesTrackChanged
    let subtitles_track = SubtitlesTrackChanged {
        video_id: "dQw4w9WgXcQ".to_string(),
    };

    let _event = LoungeEvent::SubtitlesTrackChanged(subtitles_track);
}

// Test LoungeEvent patterns
#[test]
fn test_lounge_event_matching() {
    // Create sample video data
    let video_data = VideoData {
        video_id: "".to_string(),
        author: "".to_string(),
        title: "".to_string(),
        is_playable: false,
    };

    // Test that we can match on each event type
    let events = vec![
        LoungeEvent::SessionEstablished,
        LoungeEvent::StateChange(PlaybackState {
            state: 0,
            current_time: 0.0,
            duration: 0.0,
            seekable_start_time: 0.0,
            seekable_end_time: 0.0,
            video_id: "".to_string(),
            volume: 0,
            muted: false,
            video_data: video_data.clone(),
        }),
        LoungeEvent::NowPlaying(NowPlaying {
            video_id: "".to_string(),
            current_time: 0.0,
            list_id: None,
            state: 0,
            video_data,
        }),
        LoungeEvent::LoungeStatus(vec![]),
        LoungeEvent::ScreenDisconnected,
        LoungeEvent::Unknown("".to_string()),
        LoungeEvent::AdStateChange(AdState {
            content_video_id: "".to_string(),
            is_skip_enabled: false,
        }),
        LoungeEvent::SubtitlesTrackChanged(SubtitlesTrackChanged {
            video_id: "".to_string(),
        }),
    ];

    // Just test that we can match on each type
    for event in events {
        match event {
            LoungeEvent::SessionEstablished => {}
            LoungeEvent::StateChange(_) => {}
            LoungeEvent::NowPlaying(_) => {}
            LoungeEvent::LoungeStatus(_) => {}
            LoungeEvent::ScreenDisconnected => {}
            LoungeEvent::Unknown(_) => {}
            LoungeEvent::AdStateChange(_) => {}
            LoungeEvent::SubtitlesTrackChanged(_) => {}
        }
    }
}
