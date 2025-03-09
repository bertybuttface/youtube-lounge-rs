# YouTube Lounge API Commands Reference

## Remote commands
Common format:
```
POST https://www.youtube.com/api/lounge/bc/bind?RID=rid&VER=8&CVER=1&gsessionid=session&SID=sid&auth_failure_option=send_error
content-type:application/x-www-form-urlencoded
reqX__sc=commandName
```

Each command uses `reqX__sc` parameter (usually `req0__sc`) to specify which command to execute, plus additional parameters specific to each command.

 - Video cast (`setPlaylist`)
```
req0__sc=setPlaylist
&req0_prioritizeMobileSenderPlaybackStateOnConnection=true
&req0_currentIndex=-1
&req0_videoId=xxx
&req0_listId=
&req0_currentTime=0
&req0_audioOnly=false
&req0_params=
&req0_playerParams=
&count=1
```

 - Video Queue (`addVideo`)
```
req0__sc=addVideo
&req0_videoId=qZHawOLfEG0
&req0_videoSources=XX
&count=1
```

 - Seek (`seekTo`)
```
req0__sc=seekTo
&count=1
&req0_newTime=123 // in seconds
```

 - Volume (`setVolume`)
```
req0__sc=setVolume
&req0_volume=50 // in percentage; 0-100
&count=1
```

 - Video Pause (`pause`)
```
req0__sc=pause
&count=1
```

 - Video Play (`play`)
```
&req0__sc=play
&count=1
```

 - Skip ad (`skipAd`)
```
req0__sc=skipAd
&count=1
```

 - Set autoplay (`setAutoplayMode`)
```
req0__sc=setAutoplayMode
&autoplayMode=ENABLED / ENABLED or DISABLED
&count=1
```