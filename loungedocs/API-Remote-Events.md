# YouTube Lounge API Event Flow and ID Mapping

This document details the observed event flow and ID relationships in the YouTube Lounge API based on actual debug data.

## Event Sequence

### 1. Initial Connection Sequence

- First, a `playlistModified` event is received with playlist context:

```json
{
  "currentIndex": "0",
  "firstVideoId": "dQw4w9WgXcQ",
  "listId": "RQuKt0IBC5t_IZAGghtcSFlbJNXrk",
  "videoId": "dQw4w9WgXcQ"
}
```

- Then, a `loungeStatus` event containing connected devices and queueId:

```json
{
  "devices": "[{\"app\":\"web\",...,\"name\":\"Rust Lounge Client\",\"id\":\"dn93hla9bmetrmrovfbq191sd3\",\"type\":\"REMOTE_CONTROL\"}]",
  "queueId": "RQuKt0IBC5t_IZAGghtcSFlbJNXrk"
}
```

- At this point, the system creates a mapping between list_id and device_id:

```
DEBUG: Added mapping: list_id RQuKt0IBC5t_IZAGghtcSFlbJNXrk -> device_id dn93hla9bmetrmrovfbq191sd3
```

### 2. Playback Start Sequence

- When video playback begins, a `nowPlaying` event is received without a CPN:

```json
{
  "currentTime": "0",
  "duration": "0",
  "listId": "RQuKt0IBC5t_IZAGghtcSFlbJNXrk",
  "loadedTime": "0",
  "seekableEndTime": "0",
  "seekableStartTime": "0",
  "state": "3",
  "videoId": "dQw4w9WgXcQ"
}
```

- Then an `onStateChange` event appears that includes the crucial CPN:

```json
{
  "cpn": "0Cxkp2Od9KEyzgdu",
  "currentTime": "0",
  "duration": "0",
  "loadedTime": "0",
  "seekableEndTime": "0",
  "seekableStartTime": "0",
  "state": "3"
}
```

- At this point, a PlaybackSession is created, but it's missing the videoId

- Shortly after, a complete `nowPlaying` event arrives with both CPN and video details:

```json
{
  "cpn": "0Cxkp2Od9KEyzgdu",
  "currentTime": "0.631",
  "duration": "212.061",
  "listId": "RQuKt0IBC5t_IZAGghtcSFlbJNXrk",
  "loadedTime": "18.04",
  "mdxExpandedReceiverVideoIdList": "dQw4w9WgXcQ",
  "seekableEndTime": "212.04",
  "seekableStartTime": "0",
  "state": "1",
  "videoId": "dQw4w9WgXcQ"
}
```

- The system updates the session with device information:

```
DEBUG: Updating session for CPN 0Cxkp2Od9KEyzgdu with device_id dn93hla9bmetrmrovfbq191sd3 from list_id mapping
```

## Key ID Relationships

### 1. Device ID → Queue ID (ListID) Mapping

- The library maintains a mapping from device IDs to queue/list IDs
- When device IDs change, they're tracked by queue ID

### 2. CPN → Session Mapping

- The CPN (Client Playback Nonce) is the primary key for tracking a session
- A session is initially created with a CPN but may be missing other details

### 3. Queue ID → Device ID → Session

When a new `nowPlaying` event arrives with a CPN and listId, the system:

- Finds which device is associated with that list/queue
- Associates that device's ID with the session matching the CPN

### 4. Video ID Tracking

- `videoId` appears in multiple events and is used to identify content
- `mdxExpandedReceiverVideoIdList` provides historical video context

## Important Notes

1. **Event Order**: Events don't always arrive in the intuitive order. The CPN sometimes appears in `onStateChange` before it shows up in `nowPlaying`.

2. **Device Information**: Not all devices include deviceInfo - remote control devices often have empty deviceInfo_raw strings, which can cause deserialization errors.

3. **ID Relationships**: The most reliable chain is:

   ```
   listId (queueId) → device_id → session (identified by CPN)
   ```

This mapping system allows proper attribution of video sessions to specific devices, even as connections change.
