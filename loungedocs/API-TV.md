# YouTube TV API: Differences and Structure  

The **YouTube TV API** shares similarities with YouTube’s web API but includes **TV-optimized endpoints** and a **session-based authentication model**. This page provides an overview of its **command structure, session persistence, and key differences**.  

---

## 1. **TV API vs. Web API**  

| Feature          | Web API        | TV API |
|-----------------|---------------|--------|
| Authentication  | OAuth tokens   | Session-based |
| Device Linking | Limited        | Persistent Lounge API integration |
| API Calls       | REST           | Long polling |

---

## 2. **TV API Request Structure**  

Most TV API requests are sent to:  

```http
POST https://www.youtube.com/api/lounge/bc/bind
```

Common headers:  

```http
User-Agent: Mozilla/5.0 (Linux; Android 9; Cobalt/20.0)
Content-Type: application/x-www-form-urlencoded
```

---

## 3. **Persistent Session Management**  

The TV API maintains **long-lived sessions** by:  

- Storing **session tokens** locally.  
- Refreshing **lounge tokens** automatically.  
- Using **heartbeat requests** to prevent session expiry.  

This allows uninterrupted playback across multiple device interactions.  

---

## 4. **Remote Commands via TV API**  

The TV API supports remote control commands similar to the Lounge API. Example:  

```http
POST https://www.youtube.com/api/lounge/bc/bind  
Content-Type: application/x-www-form-urlencoded  

req0__sc=play  
&req0_videoId=VIDEO_ID  
```

---

## 5. **Key Differences in Playback Handling**  

The TV API:  

- Handles **playlists differently** (managed directly by the session).  
- Uses **device registration instead of direct logins**.  
- Supports **extended autoplay settings** via API calls.  

---

## Next Steps  

- Explore **TV-specific playlist handling**.  
- Analyze **session persistence mechanisms** in depth.  
- Investigate **TV API’s remote interaction capabilities**.
