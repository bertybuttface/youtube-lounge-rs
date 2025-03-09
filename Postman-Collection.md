# YouTube Lounge API Postman Collection

This document provides a Postman Collection format for the YouTube Lounge API. You can import this into Postman to test and interact with the API.

## Collection Overview

```json
{
  "info": {
    "name": "YouTube Lounge API",
    "description": "A collection for interacting with the YouTube Lounge API for casting and controlling playback",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [
    {
      "name": "Authentication",
      "item": [
        {
          "name": "Get Screen by Pairing Code",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/pairing/get_screen",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "pairing", "get_screen"]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "pairing_code",
                  "value": "{{pairing_code}}",
                  "description": "12-digit code from TV"
                }
              ]
            },
            "description": "Obtain a lounge token by using a pairing code from the TV"
          }
        },
        {
          "name": "Refresh Lounge Token",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/pairing/get_lounge_token_batch",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "pairing", "get_lounge_token_batch"]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "screen_ids",
                  "value": "{{screen_id}}",
                  "description": "Comma-separated screen IDs"
                }
              ]
            },
            "description": "Refresh lounge tokens for one or more screens"
          }
        }
      ]
    },
    {
      "name": "Session Management",
      "item": [
        {
          "name": "Establish Connection",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID=1&VER=8&CVER=1&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "1"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "app",
                  "value": "web"
                },
                {
                  "key": "mdx-version",
                  "value": "3"
                },
                {
                  "key": "name",
                  "value": "{{device_name}}"
                },
                {
                  "key": "id",
                  "value": ""
                },
                {
                  "key": "device",
                  "value": "REMOTE_CONTROL"
                },
                {
                  "key": "capabilities",
                  "value": "que,dsdtr,atp"
                },
                {
                  "key": "method",
                  "value": "setPlaylist"
                },
                {
                  "key": "magnaKey",
                  "value": "cloudPairedDevice"
                },
                {
                  "key": "ui",
                  "value": ""
                },
                {
                  "key": "deviceContext",
                  "value": "user_agent=dunno&window_width_points=&window_height_points=&os_name=android&ms="
                },
                {
                  "key": "theme",
                  "value": "cl"
                }
              ]
            },
            "description": "Create a connection to the lounge"
          }
        },
        {
          "name": "Initialize Session",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "name",
                  "value": "{{device_name}}"
                },
                {
                  "key": "app",
                  "value": "web"
                },
                {
                  "key": "loungeIdToken",
                  "value": "{{lounge_token}}"
                }
              ]
            },
            "description": "Initialize a session to receive events"
          }
        },
        {
          "name": "Poll for Events",
          "request": {
            "method": "GET",
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?SID={{sid}}&gsessionid={{gsession}}&loungeIdToken={{lounge_token}}&CI=1&TYPE=xmlhttp&AID={{last_event_id}}",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "loungeIdToken",
                  "value": "{{lounge_token}}"
                },
                {
                  "key": "CI",
                  "value": "1"
                },
                {
                  "key": "TYPE",
                  "value": "xmlhttp"
                },
                {
                  "key": "AID",
                  "value": "{{last_event_id}}"
                }
              ]
            },
            "description": "Long-polling request to get events from the lounge"
          }
        },
        {
          "name": "Disconnect",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "ui",
                  "value": ""
                },
                {
                  "key": "TYPE",
                  "value": "terminate"
                },
                {
                  "key": "clientDisconnectReason",
                  "value": "MDX_SESSION_DISCONNECT_REASON_DISCONNECTED_BY_USER"
                }
              ]
            },
            "description": "Disconnect from the lounge"
          }
        }
      ]
    },
    {
      "name": "Remote Commands",
      "item": [
        {
          "name": "Play Video",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "req0_prioritizeMobileSenderPlaybackStateOnConnection",
                  "value": "true"
                },
                {
                  "key": "req0_currentIndex",
                  "value": "-1"
                },
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0_videoId",
                  "value": "{{video_id}}"
                },
                {
                  "key": "req0_listId",
                  "value": ""
                },
                {
                  "key": "req0_currentTime",
                  "value": "0"
                },
                {
                  "key": "req0__sc",
                  "value": "setPlaylist"
                },
                {
                  "key": "req0_audioOnly",
                  "value": "false"
                },
                {
                  "key": "req0_params",
                  "value": ""
                },
                {
                  "key": "req0_playerParams",
                  "value": ""
                }
              ]
            },
            "description": "Cast a video to the screen"
          }
        },
        {
          "name": "Queue Video",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0_videoId",
                  "value": "{{video_id}}"
                },
                {
                  "key": "req0_videoSources",
                  "value": "XX"
                },
                {
                  "key": "req0__sc",
                  "value": "addVideo"
                }
              ]
            },
            "description": "Add a video to the queue"
          }
        },
        {
          "name": "Seek",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0_newTime",
                  "value": "{{time_seconds}}"
                },
                {
                  "key": "req0__sc",
                  "value": "seekTo"
                }
              ]
            },
            "description": "Seek to a specific time in the video"
          }
        },
        {
          "name": "Set Volume",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0_volume",
                  "value": "{{volume_percent}}"
                },
                {
                  "key": "req0__sc",
                  "value": "setVolume"
                }
              ]
            },
            "description": "Set the volume (0-100)"
          }
        },
        {
          "name": "Pause",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0__sc",
                  "value": "pause"
                }
              ]
            },
            "description": "Pause video playback"
          }
        },
        {
          "name": "Play",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0__sc",
                  "value": "play"
                }
              ]
            },
            "description": "Resume/play video"
          }
        },
        {
          "name": "Skip Ad",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "req0__sc",
                  "value": "skipAd"
                }
              ]
            },
            "description": "Skip the current advertisement"
          }
        },
        {
          "name": "Set Autoplay Mode",
          "request": {
            "method": "POST",
            "header": [
              {
                "key": "Content-Type",
                "value": "application/x-www-form-urlencoded"
              }
            ],
            "url": {
              "raw": "https://www.youtube.com/api/lounge/bc/bind?RID={{rid}}&VER=8&CVER=1&gsessionid={{gsession}}&SID={{sid}}&auth_failure_option=send_error",
              "protocol": "https",
              "host": ["www", "youtube", "com"],
              "path": ["api", "lounge", "bc", "bind"],
              "query": [
                {
                  "key": "RID",
                  "value": "{{rid}}"
                },
                {
                  "key": "VER",
                  "value": "8"
                },
                {
                  "key": "CVER",
                  "value": "1"
                },
                {
                  "key": "gsessionid",
                  "value": "{{gsession}}"
                },
                {
                  "key": "SID",
                  "value": "{{sid}}"
                },
                {
                  "key": "auth_failure_option",
                  "value": "send_error"
                }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                {
                  "key": "count",
                  "value": "1"
                },
                {
                  "key": "autoplayMode",
                  "value": "ENABLED"
                },
                {
                  "key": "req0__sc",
                  "value": "setAutoplayMode"
                }
              ]
            },
            "description": "Enable or disable autoplay (ENABLED or DISABLED)"
          }
        }
      ]
    }
  ],
  "variable": [
    {
      "key": "pairing_code",
      "value": "",
      "description": "12-digit TV pairing code"
    },
    {
      "key": "lounge_token",
      "value": "",
      "description": "Authentication token for lounge"
    },
    {
      "key": "screen_id",
      "value": "",
      "description": "ID of the screen device"
    },
    {
      "key": "device_name",
      "value": "Postman Client",
      "description": "Name for this device"
    },
    {
      "key": "sid",
      "value": "",
      "description": "Session ID from initial connection"
    },
    {
      "key": "gsession",
      "value": "",
      "description": "Session value from initial connection"
    },
    {
      "key": "rid",
      "value": "1",
      "description": "Request ID counter"
    },
    {
      "key": "last_event_id",
      "value": "0",
      "description": "Last known event ID"
    },
    {
      "key": "video_id",
      "value": "",
      "description": "YouTube video ID to play"
    },
    {
      "key": "time_seconds",
      "value": "0",
      "description": "Timestamp in seconds for seek"
    },
    {
      "key": "volume_percent",
      "value": "50",
      "description": "Volume level (0-100)"
    }
  ]
}
```

## Usage Instructions

1. **Import the collection:**
   - In Postman, click "Import" and paste the JSON above
   - Or save it as a JSON file and import that file

2. **Set up environment variables:**
   - Create a new environment in Postman
   - Add variables for: pairing_code, lounge_token, screen_id, sid, gsession, etc.

3. **Authentication flow:**
   - Use "Get Screen by Pairing Code" request with a TV code
   - Save the returned lounge_token, screen_id to your environment variables

4. **Session establishment:**
   - Run "Establish Connection"
   - Run "Initialize Session"
   - From the response, extract the SID and gsession values to your environment variables

5. **Command execution:**
   - Increment the rid variable for each new command
   - Execute video control commands as needed

6. **Event tracking:**
   - Use the "Poll for Events" request to receive updates
   - Update the last_event_id after each response

7. **Disconnecting:**
   - Use the "Disconnect" request when done

## Notes

- Remember to increment the RID value for each new remote command
- Update the AID value with the latest event ID when polling for events
- The responses to event polling may be in the special chunked format described in the [Remote API Core](API-Remote-Core.md) documentation