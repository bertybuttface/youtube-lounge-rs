use youtube_lounge_rs::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, HasPreviousNextChanged,
    PlaylistModified, SubtitlesTrackChanged, VideoData, VideoQualityChanged,
};
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

    // Test AutoplayModeChanged
    let autoplay_mode = AutoplayModeChanged {
        autoplay_mode: "ENABLED".to_string(),
    };

    let _event = LoungeEvent::AutoplayModeChanged(autoplay_mode);

    // Test HasPreviousNextChanged
    let has_prev_next = HasPreviousNextChanged {
        has_next: "true".to_string(),
        has_previous: "false".to_string(),
    };

    let _event = LoungeEvent::HasPreviousNextChanged(has_prev_next);

    // Test VideoQualityChanged
    let video_quality = VideoQualityChanged {
        available_quality_levels: "[0,1080,720,480,360,240,144]".to_string(),
        quality_level: "1080".to_string(),
        video_id: "dQw4w9WgXcQ".to_string(),
    };

    let _event = LoungeEvent::VideoQualityChanged(video_quality);

    // Test AudioTrackChanged
    let audio_track = AudioTrackChanged {
        audio_track_id: "und".to_string(),
        video_id: "dQw4w9WgXcQ".to_string(),
    };

    let _event = LoungeEvent::AudioTrackChanged(audio_track);

    // Test PlaylistModified
    let playlist = PlaylistModified {
        current_index: "0".to_string(),
        first_video_id: "dQw4w9WgXcQ".to_string(),
        list_id: "RQdkpuO9KNHXPCTY5ouk6z1Yjc3sQ".to_string(),
        video_id: "dQw4w9WgXcQ".to_string(),
    };

    let _event = LoungeEvent::PlaylistModified(playlist);

    // Test AutoplayUpNext
    let up_next = AutoplayUpNext {
        video_id: "g9uJeLJCG3E".to_string(),
    };

    let _event = LoungeEvent::AutoplayUpNext(up_next);
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
        LoungeEvent::AutoplayModeChanged(AutoplayModeChanged {
            autoplay_mode: "".to_string(),
        }),
        LoungeEvent::HasPreviousNextChanged(HasPreviousNextChanged {
            has_next: "".to_string(),
            has_previous: "".to_string(),
        }),
        LoungeEvent::VideoQualityChanged(VideoQualityChanged {
            available_quality_levels: "".to_string(),
            quality_level: "".to_string(),
            video_id: "".to_string(),
        }),
        LoungeEvent::AudioTrackChanged(AudioTrackChanged {
            audio_track_id: "".to_string(),
            video_id: "".to_string(),
        }),
        LoungeEvent::PlaylistModified(PlaylistModified {
            current_index: "".to_string(),
            first_video_id: "".to_string(),
            list_id: "".to_string(),
            video_id: "".to_string(),
        }),
        LoungeEvent::AutoplayUpNext(AutoplayUpNext {
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
            LoungeEvent::AutoplayModeChanged(_) => {}
            LoungeEvent::HasPreviousNextChanged(_) => {}
            LoungeEvent::VideoQualityChanged(_) => {}
            LoungeEvent::AudioTrackChanged(_) => {}
            LoungeEvent::PlaylistModified(_) => {}
            LoungeEvent::AutoplayUpNext(_) => {}
        }
    }
}

#[test]
fn test_ad_utility_methods() {
    // Test AdState methods
    let ad_state = AdState {
        content_video_id: "content123".to_string(),
        is_skip_enabled: true,
    };

    assert!(ad_state.is_skippable());
    assert_eq!(ad_state.get_content_video_id(), "content123");

    // Test non-skippable ad
    let non_skippable_ad = AdState {
        content_video_id: "content456".to_string(),
        is_skip_enabled: false,
    };

    assert!(!non_skippable_ad.is_skippable());

    // Test LoungeEvent ad-related methods
    let ad_event = LoungeEvent::AdStateChange(ad_state);
    let state_event = LoungeEvent::StateChange(PlaybackState {
        state: 1,
        current_time: 0.0,
        duration: 0.0,
        seekable_start_time: 0.0,
        seekable_end_time: 0.0,
        video_id: "".to_string(),
        volume: 0,
        muted: false,
        video_data: VideoData {
            video_id: "".to_string(),
            author: "".to_string(),
            title: "".to_string(),
            is_playable: false,
        },
    });

    // Test is_showing_ad()
    assert!(ad_event.is_showing_ad());
    assert!(!state_event.is_showing_ad());

    // Test ad_state()
    assert!(ad_event.ad_state().is_some());
    assert!(state_event.ad_state().is_none());

    if let Some(state) = ad_event.ad_state() {
        assert!(state.is_skippable());
        assert_eq!(state.get_content_video_id(), "content123");
    }
}
