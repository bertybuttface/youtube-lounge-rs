use youtube_lounge_rs::models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, HasPreviousNextChanged,
    LoungeStatus, PlaylistModified, SubtitlesTrackChanged, VideoData, VideoQualityChanged,
    VolumeChanged,
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
        state: "1".to_string(),
        current_time: "42.5".to_string(),
        duration: "180.0".to_string(),
        seekable_start_time: "0.0".to_string(),
        seekable_end_time: "180.0".to_string(),
        video_id: "dQw4w9WgXcQ".to_string(),
        volume: "50".to_string(),
        muted: "false".to_string(),
        video_data: video_data.clone(),
        cpn: Some("test_cpn".to_string()),
        loaded_time: "60.0".to_string(),
    };

    let _event = LoungeEvent::StateChange(playback_state);

    // Test NowPlaying
    let now_playing = NowPlaying {
        video_id: "dQw4w9WgXcQ".to_string(),
        current_time: "42.5".to_string(),
        list_id: Some("PL12345".to_string()),
        state: "1".to_string(),
        video_data,
        cpn: Some("test_cpn".to_string()),
        loaded_time: "60.0".to_string(),
        duration: "180.0".to_string(),
        seekable_start_time: "0.0".to_string(),
        seekable_end_time: "180.0".to_string(),
        mdx_expanded_receiver_video_id_list: Some("abc123,def456".to_string()),
    };

    let _event = LoungeEvent::NowPlaying(now_playing);

    // Test LoungeStatus
    let devices = vec![
        Device {
            app: "YouTube".to_string(),
            name: "Device 1".to_string(),
            id: "device1".to_string(),
            device_type: "SMART_TV".to_string(),
            device_info_raw: "{}".to_string(),
            device_info: None,
        },
        Device {
            app: "YouTube".to_string(),
            name: "Device 2".to_string(),
            id: "device2".to_string(),
            device_type: "SMART_TV".to_string(),
            device_info_raw: "{}".to_string(),
            device_info: None,
        },
    ];

    let _event = LoungeEvent::LoungeStatus(devices, Some("RQ1234".to_string()));

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
        current_index: Some("0".to_string()),
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

    // Test VolumeChanged
    let volume = VolumeChanged {
        volume: "75".to_string(),
        muted: "false".to_string(),
    };

    let _event = LoungeEvent::VolumeChanged(volume);
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
            state: "0".to_string(),
            current_time: "0.0".to_string(),
            duration: "0.0".to_string(),
            seekable_start_time: "0.0".to_string(),
            seekable_end_time: "0.0".to_string(),
            video_id: "".to_string(),
            volume: "0".to_string(),
            muted: "false".to_string(),
            video_data: video_data.clone(),
            cpn: None,
            loaded_time: "0.0".to_string(),
        }),
        LoungeEvent::NowPlaying(NowPlaying {
            video_id: "".to_string(),
            current_time: "0.0".to_string(),
            list_id: None,
            state: "0".to_string(),
            video_data,
            cpn: None,
            duration: "0.0".to_string(),
            loaded_time: "0.0".to_string(),
            seekable_start_time: "0.0".to_string(),
            seekable_end_time: "0.0".to_string(),
            mdx_expanded_receiver_video_id_list: None,
        }),
        LoungeEvent::LoungeStatus(vec![], None),
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
            current_index: Some("".to_string()),
            first_video_id: "".to_string(),
            list_id: "".to_string(),
            video_id: "".to_string(),
        }),
        LoungeEvent::AutoplayUpNext(AutoplayUpNext {
            video_id: "".to_string(),
        }),
        LoungeEvent::VolumeChanged(VolumeChanged {
            volume: "50".to_string(),
            muted: "false".to_string(),
        }),
    ];

    // Just test that we can match on each type
    for event in events {
        match event {
            LoungeEvent::SessionEstablished => {}
            LoungeEvent::StateChange(_) => {}
            LoungeEvent::NowPlaying(_) => {}
            LoungeEvent::LoungeStatus(_, _) => {}
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
            LoungeEvent::VolumeChanged(_) => {}
        }
    }
}

#[test]
fn test_volume_utility_methods() {
    // Test VolumeChanged methods
    let volume_normal = VolumeChanged {
        volume: "50".to_string(),
        muted: "false".to_string(),
    };

    assert_eq!(volume_normal.volume_level(), 50);
    assert!(!volume_normal.is_muted());

    // Test muted volume
    let volume_muted = VolumeChanged {
        volume: "25".to_string(),
        muted: "true".to_string(),
    };

    assert_eq!(volume_muted.volume_level(), 25);
    assert!(volume_muted.is_muted());

    // Test invalid volume (should return 0)
    let volume_invalid = VolumeChanged {
        volume: "not_a_number".to_string(),
        muted: "false".to_string(),
    };

    assert_eq!(volume_invalid.volume_level(), 0);
    assert!(!volume_invalid.is_muted());
}

#[test]
fn test_lounge_status_queue_id() {
    // Test LoungeStatus with queue_id
    let status_with_queue = LoungeStatus {
        devices: "[]".to_string(),
        queue_id: Some("RQ1234567890".to_string()),
    };

    assert_eq!(status_with_queue.queue_id, Some("RQ1234567890".to_string()));

    // Test LoungeStatus without queue_id
    let status_without_queue = LoungeStatus {
        devices: "[]".to_string(),
        queue_id: None,
    };

    assert_eq!(status_without_queue.queue_id, None);
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
        state: "1".to_string(),
        current_time: "0.0".to_string(),
        duration: "0.0".to_string(),
        seekable_start_time: "0.0".to_string(),
        seekable_end_time: "0.0".to_string(),
        video_id: "".to_string(),
        volume: "0".to_string(),
        muted: "false".to_string(),
        loaded_time: "0.0".to_string(),
        cpn: None,
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
