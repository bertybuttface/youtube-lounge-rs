# YouTube Remote API Core

## Basic Definitions
- **screen**: The TV/device one is trying to cast to
- **remote**: The device used to control what's being cast to the screen
- **device**: Screen or remote device
- **lounge**: The definition of a "room" in the API - said room being composed of a screen and remotes
- **event**: An object returned by the API to indicate something changed in the lounge (something connected, something is playing, etc.)
- **chunk**: A collection of events

## Authentication

### Obtaining a Lounge Token
The first step for using the API is getting a lounge token. There are two methods:

1. **Discovery via DIAL protocol** (described in the [Discovery](Discovery.md) page)
2. **Pairing via a TV code**:
   - In the YouTube app settings, find the "Link with TV code" option
   - Use the 12-digit code in a request:
   ```
   POST https://www.youtube.com/api/lounge/pairing/get_screen
   content-type: application/x-www-form-urlencoded
   pairing_code=793868867097
   ```
   
   Response example:
   ```json
   {
       "screen": {
           "accessType": "permanent",
           "screenId": "qweqweqwe",
           "dialAdditionalDataSupportLevel": "unknown",
           "loungeTokenRefreshIntervalMs": 1123200000,
           "loungeToken": "qweqweqwe",
           "clientName": "tvhtml5",
           "name": "YouTube on TV",
           "expiration": 1550144489377,
           "deviceId": "rergergergerg"
       }
   }
   ```

When pairing with a code, save the response locally for reconnection. The device ID serves as a unique identifier.

### Refreshing the Lounge Token
Lounge tokens expire periodically. For discoverable devices, rediscover and query the screen. For code-paired screens, use:

```
POST https://www.youtube.com/api/lounge/pairing/get_lounge_token_batch
content-type: application/x-www-form-urlencoded
screen_ids=abc,123
```

Response:
```json
{
    "screens": [
        {
            "screenId": "qweqwe",
            "refreshIntervalInMillis": 1123200000,
            "remoteRefreshIntervalMs": 79200000,
            "refreshIntervalMs": 1123200000,
            "loungeTokenLifespanMs": 1209600000,
            "loungeToken": "gregreg",
            "remoteRefreshIntervalInMillis": 79200000,
            "expiration": 1654079296252
        }
    ]
}
```

You can batch request lounge tokens by using multiple screen IDs separated by commas.

## Establishing a Connection
With the lounge token, create a link to the lounge so it recognizes the remote device:

```
POST https://www.youtube.com/api/lounge/bc/bind?RID=1&VER=8&CVER=1&auth_failure_option=send_error
content-type: application/x-www-form-urlencoded
{
    "app": "web",
    "mdx-version": "3",
    "name": "device_name",
    "id": "",
    "device": "REMOTE_CONTROL",
    "capabilities": "que,dsdtr,atp",
    "method": "setPlaylist",
    "magnaKey": "cloudPairedDevice",
    "ui": "",
    "deviceContext": "user_agent=dunno&window_width_points=&window_height_points=&os_name=android&ms=",
    "theme": "cl"
}
```

The body should be form data encoded, though shown as JSON above for clarity.

After this request, you should see "xxx is now connected" (xxx being the device_name) on the screen. This request also creates a private playlist for queuing and playing videos.

Note: The parameters `capabilities`, `magnaKey`, `ui`, and `theme` are not fully understood but are required for functionality.

## Maintaining the Session
To track lounge events (what's playing, connections, pauses, ads, etc.), use long HTTP polling. First, establish a session:

```
POST https://www.youtube.com/api/lounge/bc/bind
content-type: application/x-www-form-urlencoded
name=devicename&app=app_name&loungeIdToken=loungeToken
```

### Event Format
Events are returned in chunks:
```
number
[[id, ["event_name", event_argument, not sure what]
]
```

Where:
- The first number indicates how many characters are in the following chunk
- `id` values are auto-incrementing starting from 0
- `event_name` identifies the event type
- `event_argument` may or may not exist depending on the event
- The format is similar to JSONP

The first response contains essential session variables:
```
234
[[0,["c","FEWWEFWEFWEF","",8]]
,[1,["S","wefwefwef"]]
,[2,["loungeStatus",{}]
,[3,["playlistModified",{"videoIds":""}]]
,[4,["onAutoplayModeChanged",{"autoplayMode":"UNSUPPORTED"}]]
,[5,["onPlaylistModeChanged",{"shuffleEnabled":"false","loopEnabled":"false"}]]
]
```

Important variables from this response:
- `SID`: From event "c"
- `gsession`: From event "S"

### Continued Event Polling
For subsequent event polling:
```
GET https://www.youtube.com/api/lounge/bc/bind?SID=sid&gsessionid=gsession&loungeIdToken=loungeToken&CI=1&TYPE=xmlhttp&AID=???
```

The `AID` parameter should be the last known event ID, allowing you to get all events since that ID.

Key state variables to track:
- `SID`: From the first events chunk
- `gsession`: From the first events chunk
- `AID`: Last known event ID
- `loungeToken`: Authentication token

## Remote Commands
To control the screen (like playing a video):

```
POST https://www.youtube.com/api/lounge/bc/bind?RID=???????????&VER=8&CVER=1&gsessionid=session&SID=sid&auth_failure_option=send_error
content-type: application/x-www-form-urlencoded
req0_prioritizeMobileSenderPlaybackStateOnConnection=true
&req0_currentIndex=-1
&count=1
&req0_videoId=xxx
&req0_listId=
&req0_currentTime=0
&req0__sc=setPlaylist
&req0_audioOnly=false
&req0_params=
&req0_playerParams=
```

All remote commands include the `req0__sc` parameter, which indicates the command type.

Additional state variables for commands:
- `RID`: Remote command ID (auto-incremented for each successful command)
- `req0_`, `req1_`, etc.: Potentially for batching commands

Note: It's theorized but not confirmed that the `reqX_` format allows batching multiple commands in one request.

## Disconnecting
To disconnect from the lounge:

```
POST https://www.youtube.com/api/lounge/bc/bind?RID=x&VER=8&CVER=1&gsessionid=session&SID=sid&auth_failure_option=send_error
content-type: application/x-www-form-urlencoded
ui=&TYPE=terminate&clientDisconnectReason=MDX_SESSION_DISCONNECT_REASON_DISCONNECTED_BY_USER
```

This should display "xxx disconnected" on the screen.