mod codec;
pub use codec::LoungeCodec;
mod commands;
pub use commands::PlaybackCommand;
mod error;
pub use error::LoungeError;
mod events;
pub use events::{LoungeEvent, PlaybackSession, PlaybackStatus};
mod models;
pub use models::{
    AdState, AudioTrackChanged, AutoplayModeChanged, AutoplayUpNext, Device, DeviceInfo,
    HasPreviousNextChanged, LoungeStatus, NowPlaying, PlaybackState, PlaylistModified, Screen,
    ScreenResponse, ScreensResponse, SubtitlesTrackChanged, VideoData, VideoQualityChanged,
    VolumeChanged,
};
mod state;
use state::{ConnectionState, ConnectionStatus, InnerState, SessionState};
mod utils;
pub use utils::youtube_parse;

use bytes::BytesMut;
use futures::{FutureExt, StreamExt}; // Needed for response.bytes_stream()
use reqwest::Client;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use tokio::sync::{broadcast, watch, Notify, RwLock}; // Added watch
use tokio::time::{sleep, timeout, Duration};
use tokio_util::codec::Decoder;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid; // Needed for jitter

const BUFFER_CAPACITY: usize = 16 * 1024; // 16KB initial buffer capacity
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(32); // Wait 32s for next chunk
const MIN_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10); // Timeout for establishing HTTP connection
const LONG_POLL_TIMEOUT: Duration = Duration::from_secs(300); // Overall timeout for the long poll request

// Type alias for the optional callback function pointer for clarity
pub type TokenCallback = Option<Box<dyn Fn(&str, &str) + Send + Sync + 'static>>;

/// Main client enables controlling YouTube playback on TV devices through
/// the YouTube Lounge API protocol. It handles pairing, authentication,
/// session management, and sending commands to control playback.
///
/// # Logging
///
/// This library uses the `tracing` crate for logging. To enable logs, you'll need to
/// initialize a tracing subscriber in your application.
///
/// Example using `tracing_subscriber`:
/// ```no_run
/// use tracing::Level;
/// use tracing_subscriber::FmtSubscriber;
///
/// // Create a subscriber with the desired log level
/// let subscriber = FmtSubscriber::builder()
///     .with_max_level(Level::DEBUG) // Set to DEBUG, INFO, WARN, or ERROR
///     .finish();
///
/// // Initialize the global subscriber
/// tracing::subscriber::set_global_default(subscriber)
///     .expect("Failed to set tracing subscriber");
/// ```
///
/// The log levels control what information is displayed:
/// - `TRACE`: Shows all logs, including detailed internal operations
/// - `DEBUG`: Shows detailed information about network requests, message parsing, etc.
/// - `INFO`: Shows high-level operations and successful connections
/// - `WARN`: Shows warnings and non-critical errors
/// - `ERROR`: Shows critical failures and error conditions
struct ConnectionManagerContext {
    client: Arc<Client>,
    screen_id: String,
    device_name: String,
    device_id: String,
    shared_state: Arc<RwLock<InnerState>>,
    session_state_rwlock: Arc<RwLock<SessionState>>,
    event_sender: broadcast::Sender<LoungeEvent>,
    latest_now_playing: Arc<RwLock<Option<NowPlaying>>>,
    aid_atomic: Arc<AtomicU32>,
    shutdown_notify: Arc<Notify>,
    state_tx: Arc<watch::Sender<ConnectionState>>, // Also pass state sender for potential internal updates
}

pub struct LoungeClient {
    client: Arc<Client>,
    device_id: String,
    screen_id: String,
    device_name: String,
    // Changed SessionState to be Arc<RwLock<>> for sharing with manager task
    session_state: Arc<RwLock<SessionState>>,
    event_sender: broadcast::Sender<LoungeEvent>,
    shared_state: Arc<RwLock<InnerState>>, // Contains lounge_token and callback
    aid_atomic: Arc<AtomicU32>,
    // Flag to signal the connection manager task to stop
    stop_signal: Arc<AtomicBool>,
    // JoinHandle for the management task
    management_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    // Shutdown notifier for the management task
    shutdown_notify: Arc<Notify>,
    // Watch channel for observing the connection state
    connection_state_tx: Arc<watch::Sender<ConnectionState>>,
    connection_state_rx: watch::Receiver<ConnectionState>,
}

impl LoungeClient {
    /// Create a new LoungeClient. If a device_id is provided, it will be used;
    /// otherwise, a new UUID is generated. Optionally accepts a custom reqwest client
    /// for connection reuse and shared configuration.
    pub fn new(
        screen_id: &str,
        lounge_token: &str,
        device_name: &str,
        device_id: Option<&str>,
        custom_client: Option<Arc<Client>>,
    ) -> Self {
        let client = custom_client.unwrap_or_else(|| {
            Arc::new(
                Client::builder()
                    .pool_idle_timeout(Some(Duration::from_secs(600)))
                    .pool_max_idle_per_host(256)
                    .timeout(REQUEST_TIMEOUT) // Default request timeout
                    .connect_timeout(REQUEST_TIMEOUT) // Connection timeout
                    .build()
                    .unwrap(),
            )
        });
        let device_id = device_id.map_or_else(|| Uuid::new_v4().to_string(), ToString::to_string);
        let (event_tx, _) = broadcast::channel(100);
        let (state_tx, state_rx) = watch::channel(ConnectionState::Disconnected);

        // Initialize the inner state for the Mutex
        let initial_state = InnerState {
            lounge_token: lounge_token.to_string(),
            token_refresh_callback: None, // Will be set later via method
        };

        Self {
            client,
            device_id,
            screen_id: screen_id.to_string(),
            device_name: device_name.to_string(),
            session_state: Arc::new(RwLock::new(SessionState::new())),
            shared_state: Arc::new(RwLock::new(initial_state)),
            event_sender: event_tx,
            connection_state_tx: Arc::new(state_tx),
            connection_state_rx: state_rx,
            management_task: Arc::new(RwLock::new(None)),
            shutdown_notify: Arc::new(Notify::new()),
            aid_atomic: Arc::new(AtomicU32::new(0)),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn set_token_refresh_callback<F>(&self, callback: F)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        let mut state_guard = self.shared_state.write().await;
        state_guard.token_refresh_callback = Some(Box::new(callback));
        debug!("Token refresh callback set.");
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn screen_id(&self) -> &str {
        &self.screen_id
    }

    pub fn event_receiver(&self) -> broadcast::Receiver<LoungeEvent> {
        self.event_sender.subscribe()
    }

    /// Get the current state of the connection manager.
    pub fn current_state(&self) -> ConnectionState {
        self.connection_state_rx.borrow().clone()
    }

    /// Pair with a screen using a pairing code displayed on the TV
    pub async fn pair_with_screen(pairing_code: &str) -> Result<Screen, LoungeError> {
        info!("Pairing with screen using code: {}", pairing_code);
        let client = Client::new();
        let params = [("pairing_code", pairing_code)];

        let response = client
            .post("https://www.youtube.com/api/lounge/pairing/get_screen")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_msg = format!("Failed to pair with screen: {}", response.status());
            error!("{}", error_msg);
            return Err(LoungeError::InvalidResponse(error_msg));
        }

        let screen_response = response.json::<ScreenResponse>().await?;
        info!(
            "Successfully paired with screen: {}",
            screen_response
                .screen
                .name
                .as_deref()
                .unwrap_or("<unnamed>")
        );
        Ok(screen_response.screen)
    }

    pub async fn refresh_lounge_token(screen_id: &str) -> Result<Screen, LoungeError> {
        info!("Refreshing lounge token for screen_id: {}", screen_id);
        let client = Client::new();
        let params = [("screen_ids", screen_id)];

        let response = client
            .post("https://www.youtube.com/api/lounge/pairing/get_lounge_token_batch")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let error_msg = format!("Failed to refresh token: {}: {}", status, body_text);
            error!("{}", error_msg);
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(LoungeError::TokenExpired);
            }
            return Err(LoungeError::InvalidResponse(error_msg));
        }

        let screens_response = response.json::<ScreensResponse>().await?;

        let screen = screens_response
            .screens
            .into_iter()
            .next()
            .ok_or_else(|| LoungeError::InvalidResponse("No screens returned".to_string()))?;

        debug!(
            "Token refreshed successfully for screen: {}",
            screen.name.as_deref().unwrap_or("<unnamed>")
        );

        Ok(screen)
    }

    /// Check if a screen is available using the current lounge token
    pub async fn check_screen_availability(&self) -> Result<bool, LoungeError> {
        debug!(
            "Checking screen availability for screen_id: {}",
            self.screen_id
        );

        let token = {
            let state_guard = self.shared_state.read().await;
            state_guard.lounge_token.clone()
        };
        let params = [("lounge_token", &token)];
        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/pairing/get_screen_availability")
            .form(&params)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            warn!("Token expired for screen_id: {}", self.screen_id);
            return Err(LoungeError::TokenExpired);
        }

        let available = response.status().is_success();
        debug!("Screen availability: {}", available);

        Ok(available)
    }

    pub async fn check_screen_availability_with_refresh(&self) -> Result<bool, LoungeError> {
        match self.check_screen_availability().await {
            Ok(available) => Ok(available),
            Err(LoungeError::TokenExpired) => {
                info!("Refreshing expired token (check_screen_availability_with_refresh)");
                let screen = Self::refresh_lounge_token(&self.screen_id).await?;
                {
                    let mut state = self.shared_state.write().await;
                    state.lounge_token = screen.lounge_token.clone();
                    debug!("Shared state updated with refreshed token.");
                    if let Some(ref callback) = state.token_refresh_callback {
                        debug!("Calling token refresh callback.");
                        callback(&self.screen_id, &screen.lounge_token);
                    }
                }
                self.check_screen_availability().await
            }
            Err(e) => Err(e),
        }
    }

    /// Attempts the initial bind request to get SID/GSessionID.
    /// Does NOT spawn the connection manager.
    async fn try_initial_bind(&self) -> Result<(String, String), LoungeError> {
        info!("Attempting initial bind for screen: {}", self.screen_id);

        let params = [
            ("RID", "1"),
            ("VER", "8"),
            ("CVER", "1"),
            ("auth_failure_option", "send_error"),
            ("TYPE", "xmlhttp"),
        ];

        let form_data = self.build_connect_form_data().await?;
        debug!(?params, "Sending initial bind request");

        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await?;

        match response.status().as_u16() {
            401 => {
                error!(
                    "Initial bind failed: 401 Unauthorized. Token is likely invalid or expired."
                );
                return Err(LoungeError::TokenExpired);
            }
            404 => {
                error!(
                    "Initial bind failed: 404 Not Found. Screen ID might be invalid or unpaired."
                );
                return Err(LoungeError::InvalidResponse(
                    "Screen not found (404)".to_string(),
                ));
            }
            status if !response.status().is_success() => {
                let body_text = response.text().await.map_err(LoungeError::RequestFailed)?;
                let error_msg = format!("Initial bind failed: {}: {}", status, body_text);
                error!("{}", error_msg);
                return Err(LoungeError::InvalidResponse(error_msg));
            }
            _ => {} // Success, proceed
        }

        let body = response.bytes().await?;

        debug!("Extracting session IDs from initial bind response");
        let (sid_opt, gsessionid_opt) = crate::utils::extract_session_ids(&body)?;

        match (sid_opt, gsessionid_opt) {
            (Some(sid), Some(gsessionid)) => {
                info!(
                    "Initial bind successful. SID: {}, GSessionID: {}",
                    sid, gsessionid
                );
                Ok((sid, gsessionid))
            }
            _ => {
                error!(
                    "Initial bind response successful, but failed to extract SID/GSessionID. Body: {:?}",
                    String::from_utf8_lossy(&body)
                );
                Err(LoungeError::InvalidResponse(
                    "Failed to extract session IDs from bind response".to_string(),
                ))
            }
        }
    }

    /// Establish the initial connection and start the background connection manager.
    pub async fn connect(&self) -> Result<(), LoungeError> {
        info!("Connecting to screen: {}", self.screen_id);

        // Clear any previous stop signal
        self.stop_signal.store(false, Ordering::SeqCst);
        // Reset the notification for a fresh start
        while self.shutdown_notify.notified().now_or_never().is_some() {}

        // Reset session state before attempting bind
        {
            let mut session_write = self.session_state.write().await;
            *session_write = SessionState::new();
            debug!("SessionState reset before initial connect attempt.");
        }
        // Set state to Connecting
        let _ = self.connection_state_tx.send(ConnectionState::Connecting);

        // Attempt the initial bind
        match self.try_initial_bind().await {
            Ok((sid, gsessionid)) => {
                // Store the new session details
                {
                    let mut session_write = self.session_state.write().await;
                    session_write.sid = Some(sid.clone());
                    session_write.gsessionid = Some(gsessionid.clone());
                    debug!("Stored new SID/GSessionID in shared SessionState.");
                }

                // Send event indicating success
                let _ = self.event_sender.send(LoungeEvent::SessionEstablished);
                // Set state to Connected *before* starting manager? Or let manager do it? Let manager do it.
                // let _ = self.connection_state_tx.send(ConnectionState::Connected);

                // Start the persistent connection manager task
                self.start_connection_manager().await; // Make async to store handle

                info!("Connection established and manager task started.");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Initial connection failed");
                let _ = self
                    .connection_state_tx
                    .send(ConnectionState::Failed(format!(
                        "Initial connection failed: {}",
                        e
                    )));
                // Don't start the manager task if initial connect fails
                Err(e)
            }
        }
    }

    /// Connect to the screen with automatic token refresh if needed.
    pub async fn connect_with_refresh(&self) -> Result<(), LoungeError> {
        match self.connect().await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                info!("Refreshing expired token (connect_with_refresh)");
                match Self::refresh_lounge_token(&self.screen_id).await {
                    Ok(screen) => {
                        // Update shared state *before* retrying connect
                        {
                            let mut state = self.shared_state.write().await;
                            state.lounge_token = screen.lounge_token.clone();
                            debug!("Shared state updated with refreshed token.");
                            if let Some(ref callback) = state.token_refresh_callback {
                                debug!("Calling token refresh callback.");
                                callback(&self.screen_id, &screen.lounge_token);
                            }
                        }
                        debug!("Retrying connect after successful token refresh.");
                        // Retry the connection attempt
                        self.connect().await
                    }
                    Err(refresh_err) => {
                        error!(error = %refresh_err, "Token refresh failed during connect_with_refresh");
                        let err = LoungeError::TokenRefreshFailed(Box::new(refresh_err));
                        let _ = self
                            .connection_state_tx
                            .send(ConnectionState::Failed(format!(
                                "Token refresh failed: {}",
                                err
                            )));
                        Err(err)
                    }
                }
            }
            Err(e) => Err(e), // Propagate other connection errors
        }
    }

    // Make async to allow storing handle
    async fn start_connection_manager(&self) {
        // Create the context struct
        let ctx = ConnectionManagerContext {
            client: self.client.clone(),
            screen_id: self.screen_id.clone(),
            device_name: self.device_name.clone(),
            device_id: self.device_id.clone(),
            shared_state: self.shared_state.clone(),
            session_state_rwlock: self.session_state.clone(),
            event_sender: self.event_sender.clone(),
            latest_now_playing: Arc::new(RwLock::new(None::<NowPlaying>)), // Create locally
            aid_atomic: self.aid_atomic.clone(),
            shutdown_notify: self.shutdown_notify.clone(),
            state_tx: self.connection_state_tx.clone(),
        };

        // Clone Arcs needed *outside* the task's main loop for storing the handle
        let stop_signal = self.stop_signal.clone();
        let management_task_arc = self.management_task.clone();

        let handle = tokio::spawn(async move {
            // state_tx, shutdown_notify moved in
            info!("Connection manager task started.");
            let _ = ctx.state_tx.send(ConnectionState::Connecting); // Initial state
            let mut backoff = MIN_BACKOFF;
            // Outer loop only breaks on explicit shutdown signal
            loop {
                // Check if termination requested
                if stop_signal.load(Ordering::Relaxed) {
                    info!("Connection manager task stopping due to stop signal.");
                    let _ = ctx.state_tx.send(ConnectionState::Stopping);
                    // Final state update before exiting
                    let _ = ctx.state_tx.send_replace(ConnectionState::Disconnected);
                    break;
                }

                // Use select! for the main operation cycle
                tokio::select! {
                    biased; // Check notification first

                    _ = ctx.shutdown_notify.notified() => { // Branch 1: Shutdown notification
                        info!("Connection manager received shutdown notification.");
                        let _ = ctx.state_tx.send(ConnectionState::Stopping);
                        break; // Exit loop immediately
                    }

                    // Normal operation logic wrapped in an async block
                    _ = async {
                         // Check stop_signal *again* just in case notification was missed (belt-and-suspenders)
                        if stop_signal.load(Ordering::Relaxed) { return; }

                         // --- Read current session state ---
                         let (current_sid, current_gsessionid) = {
                             let session_read = ctx.session_state_rwlock.read().await;
                             (session_read.sid.clone(), session_read.gsessionid.clone())
                         };

                         let result = if let (Some(sid), Some(gsessionid)) =
                             (current_sid, current_gsessionid)
                         {
                             // --- State: Connected / Polling ---
                             trace!("Manager state: Polling events.");
                             let _ = ctx.state_tx.send_if_modified(|prev| if *prev != ConnectionState::Connected {*prev = ConnectionState::Connected; true} else {false} );
                             Self::poll_events(&ctx, &sid, &gsessionid).await // Pass ctx and IDs
                         } else {
                             // --- State: Disconnected / Reconnecting ---
                             debug!("Manager state: Attempting to bind session.");
                             let _ = ctx.state_tx.send_if_modified(|prev| if *prev != ConnectionState::Connecting {*prev = ConnectionState::Connecting; true} else {false} );
                             Self::attempt_bind(&ctx).await // Pass ctx
                         };

                         // --- Handle Result ---
                         match result {
                             Ok(ConnectionStatus::Success) => {
                                 // Successful poll or bind, reset backoff. State is Connected or Connecting->Connected.
                                 backoff = MIN_BACKOFF;
                             },
                             Ok(ConnectionStatus::SessionInvalidated) => {
                                 warn!("Session invalidated (e.g., 400/404/410). Clearing session state.");
                                 {
                                     let mut session_write = ctx.session_state_rwlock.write().await;
                                     session_write.sid = None;
                                     session_write.gsessionid = None;
                                 }
                                 let _ = ctx.event_sender.send(LoungeEvent::ScreenDisconnected);
                                 let _ = ctx.state_tx.send(ConnectionState::Connecting); // Will attempt to reconnect
                                 // Apply backoff before next attempt
                                 let delay_duration = calculate_backoff_delay(backoff);
                                 let _ = ctx.state_tx.send(ConnectionState::WaitingToReconnect { backoff: delay_duration });
                                 debug!("Backing off for {:?}", delay_duration);
                                 tokio::select! { // Sleep with interrupt
                                     _ = sleep(delay_duration) => {},
                                     _ = ctx.shutdown_notify.notified() => { return; } // Return from async block if interrupted
                                 }
                                 backoff = (backoff * 2).min(MAX_BACKOFF);
                             },
                             Ok(ConnectionStatus::TokenExpired) => {
                                 warn!("Token expired (401 detected). Attempting refresh.");
                                 match Self::try_refresh_token(&ctx.screen_id, &ctx.shared_state).await {
                                     Ok(()) => { info!("Token refreshed successfully."); backoff = MIN_BACKOFF; },
                                     Err(e) => {
                                         error!(error = %e, "Token refresh attempt failed.");
                                         let _ = ctx.state_tx.send(ConnectionState::Failed(format!("Token refresh failed: {}", e)));
                                         // Apply backoff before next attempt
                                         let delay_duration = calculate_backoff_delay(backoff);
                                         let _ = ctx.state_tx.send(ConnectionState::WaitingToReconnect { backoff: delay_duration });
                                         debug!("Backing off for {:?}", delay_duration);
                                         tokio::select! { // Sleep with interrupt
                                             _ = sleep(delay_duration) => {},
                                             _ = ctx.shutdown_notify.notified() => { return; } // Return from async block if interrupted
                                         }
                                         backoff = (backoff * 2).min(MAX_BACKOFF);
                                     }
                                 }
                             },
                             // ADDED: Specific handling for ConnectionClosed from poll_events
                             Err(LoungeError::ConnectionClosed) => {
                                 info!("Connection manager stopped polling due to external request (disconnect/drop).");
                                 // This error should cause the outer loop to break in the next iteration
                                 // when stop_signal is checked or shutdown_notify is selected.
                                 // We just return from the async block here.
                             }
                             Err(e) => {
                                 error!(error = %e, "Connection manager encountered an error");
                                 {
                                     let mut session_write = ctx.session_state_rwlock.write().await;
                                     if session_write.sid.is_some() {
                                         warn!("Clearing session state due to error: {}", e);
                                         session_write.sid = None;
                                         session_write.gsessionid = None;
                                         let _ = ctx.event_sender.send(LoungeEvent::ScreenDisconnected);
                                     }
                                 }
                                 // Apply backoff before next attempt
                                 let delay_duration = calculate_backoff_delay(backoff);
                                 let _ = ctx.state_tx.send(ConnectionState::WaitingToReconnect { backoff: delay_duration });
                                 debug!("Backing off for {:?}", delay_duration);
                                 tokio::select! { // Sleep with interrupt
                                     _ = sleep(delay_duration) => {},
                                     _ = ctx.shutdown_notify.notified() => { return; } // Return from async block if interrupted
                                 }
                                 backoff = (backoff * 2).min(MAX_BACKOFF);
                             },
                         }
                      } => { /* Normal async block completed */ }
                } // end select!
            } // end loop

            info!("Connection manager task finished.");
            let _ = ctx.state_tx.send_replace(ConnectionState::Disconnected); // Use replace for final state on exit
        }); // end tokio::spawn

        // Store the JoinHandle
        {
            let mut task_guard = management_task_arc.write().await;
            *task_guard = Some(handle);
            debug!("Stored management task JoinHandle.");
        }
    }

    /// Helper for the manager task to attempt a bind request.
    /// Updates the shared SessionState on success.
    async fn attempt_bind(
        ctx: &ConnectionManagerContext, // Use context struct
    ) -> Result<ConnectionStatus, LoungeError> {
        let current_lounge_token = {
            let state_guard = ctx.shared_state.read().await;
            state_guard.lounge_token.clone()
        };

        // Construct form data similar to initial connect, but using current token etc.
        let form_fields: Vec<(&str, &str)> = vec![
            ("app", "web"),
            ("mdx-version", "3"),
            ("name", &ctx.device_name),
            ("id", &ctx.device_id),
            ("device", "REMOTE_CONTROL"),
            ("capabilities", "que,dsdtr,atp"),
            ("method", "setPlaylist"),
            ("magnaKey", "cloudPairedDevice"),
            ("ui", "false"),
            ("deviceContext", "user_agent=dunno"),
            ("window_width_points", ""),
            ("window_height_points", ""),
            ("os_name", "android"),
            ("ms", ""),
            ("theme", "cl"),
            ("loungeIdToken", &current_lounge_token),
        ];
        // Use map_err to convert UrlEncodingFailed into LoungeError
        let form_data =
            serde_urlencoded::to_string(&form_fields).map_err(LoungeError::UrlEncodingFailed)?;

        // Use the current RID from shared state for the bind attempt
        let rid_val = {
            let session_read = ctx.session_state_rwlock.read().await;
            session_read.rid.fetch_add(1, Ordering::SeqCst)
        };
        let rid_string = rid_val.to_string(); // Create String for params array

        let params = [
            ("RID", rid_string.as_str()),
            ("VER", "8"),
            ("CVER", "1"),
            ("auth_failure_option", "send_error"),
            ("TYPE", "bind"),
        ];

        debug!(?params, "Attempting bind request within manager");
        // Use select! to make the send operation interruptible
        let response_result = tokio::select! {
            biased;
            _ = ctx.shutdown_notify.notified() => {
                info!("Shutdown requested during bind attempt send.");
                return Err(LoungeError::ConnectionClosed);
            }
            res = ctx.client
                    .post("https://www.youtube.com/api/lounge/bc/bind")
                    .query(&params)
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(form_data)
                    .timeout(Duration::from_secs(20))
                    .send() => res, // Result of the send future
        };

        // Handle the result of the send operation
        let response = response_result.map_err(LoungeError::RequestFailed)?;

        match response.status().as_u16() {
            200 => {
                // Also make body reading interruptible
                let body_result = tokio::select! {
                     biased;
                    _ = ctx.shutdown_notify.notified() => {
                        info!("Shutdown requested while reading bind response body.");
                        return Err(LoungeError::ConnectionClosed);
                     }
                    body_res = response.bytes() => body_res,
                };
                let body = body_result.map_err(LoungeError::RequestFailed)?;
                debug!("Bind successful, extracting session IDs.");
                // Use map_err for potential utils error
                let (sid_opt, gsessionid_opt) = crate::utils::extract_session_ids(&body)?;

                if let (Some(sid), Some(gsessionid)) = (sid_opt, gsessionid_opt) {
                    info!(
                        "Re-bind successful. New SID: {}, GSessionID: {}",
                        sid, gsessionid
                    );
                    // Update shared state
                    {
                        let mut session_write = ctx.session_state_rwlock.write().await;
                        session_write.sid = Some(sid.clone());
                        session_write.gsessionid = Some(gsessionid.clone());
                        session_write.command_offset.store(0, Ordering::SeqCst);
                        debug!("Stored new SID/GSessionID, reset offset in shared SessionState.");
                    }
                    let _ = ctx.event_sender.send(LoungeEvent::SessionEstablished);
                    // let _ = state_tx.send(ConnectionState::Connected); // Let manager loop set state
                    Ok(ConnectionStatus::Success)
                } else {
                    error!(
                        "Bind response successful (200), but failed to extract SID/GSessionID. Body: {:?}",
                        String::from_utf8_lossy(&body)
                    );
                    Err(LoungeError::InvalidResponse(
                        "Failed to extract session IDs from bind response".to_string(),
                    ))
                }
            }
            401 => {
                warn!("Bind attempt failed: 401 Unauthorized.");
                Ok(ConnectionStatus::TokenExpired)
            }
            404 => {
                error!(
                    "Bind attempt failed: 404 Not Found. Screen ID might be invalid or unpaired."
                );
                // Treat 404 as session invalidated, requires user action or very long backoff
                Ok(ConnectionStatus::SessionInvalidated)
            }
            400 | 410 => {
                let status = response.status();
                let body_text = response.text().await.map_err(LoungeError::RequestFailed)?;
                error!(%status, body=%body_text, "Terminal bind error ({})", status);
                Ok(ConnectionStatus::SessionInvalidated)
            }
            status if !response.status().is_success() => {
                let body_text = response.text().await.map_err(LoungeError::RequestFailed)?;
                let error_msg = format!("Bind attempt failed: {}: {}", status, body_text);
                error!("{}", error_msg);
                Err(LoungeError::InvalidResponse(error_msg))
            }
            _ => {
                warn!(status=%response.status(), "Unexpected successful status code during bind attempt.");
                Err(LoungeError::InvalidResponse(format!(
                    "Unexpected status {} during bind",
                    response.status()
                )))
            }
        }
    }

    /// Helper for the manager task to perform one long-polling event request.
    async fn poll_events(
        ctx: &ConnectionManagerContext, // Use context struct
        sid: &str,                      // Pass specific session IDs
        gsessionid: &str,
    ) -> Result<ConnectionStatus, LoungeError> {
        let current_lounge_token = {
            let state_guard = ctx.shared_state.read().await;
            state_guard.lounge_token.clone()
        };
        let current_aid_val = ctx.aid_atomic.load(Ordering::SeqCst);
        let aid_string = current_aid_val.to_string();

        let params = [
            ("SID", sid),
            ("gsessionid", gsessionid),
            ("RID", "rpc"),
            ("VER", "8"),
            ("v", "2"),
            ("device", "REMOTE_CONTROL"),
            ("app", "youtube-desktop"),
            ("loungeIdToken", current_lounge_token.as_str()),
            ("name", &ctx.device_name),
            ("CI", "0"),
            ("TYPE", "xmlhttp"),
            ("AID", aid_string.as_str()),
        ];

        trace!(?params, "Sending event poll request (long poll)");
        // FIX: Make the initial send() interruptible using select!
        let response_result = tokio::select! {
            biased;
            _ = ctx.shutdown_notify.notified() => {
                info!("Shutdown requested during event poll send.");
                // We need to return a Result here, signaling closure seems appropriate
                return Err(LoungeError::ConnectionClosed);
            }
            // Match the result of the send future directly
            res = ctx.client
                .get("https://www.youtube.com/api/lounge/bc/bind")
                .query(&params)
                .timeout(LONG_POLL_TIMEOUT) // Use long poll timeout
                .send() => res, // This assigns the Result<Response, reqwest::Error>
        };

        // Handle the result of the send operation, mapping potential reqwest error
        let response = match response_result {
            Ok(res) => res, // Successful send, got a Response
            Err(e) => {
                // If the error is a timeout specifically during connection/sending, handle it
                if e.is_timeout() {
                    warn!(error=%e, "Timeout sending event poll request, will retry.");
                    // Treat send timeout as a recoverable error needing backoff
                    return Err(LoungeError::RequestFailed(e));
                } else {
                    // Other send errors (DNS, connection refused, etc.)
                    error!(error = %e, "Failed to send event poll request");
                    return Err(LoungeError::RequestFailed(e));
                }
            }
        };

        // --- Check Status Codes ---
        match response.status().as_u16() {
            200 => {
                debug!(
                    "Event poll request successful ({}), processing response stream.",
                    response.status()
                );
            }
            400 | 404 | 410 => {
                let status = response.status();
                // Make text reading interruptible
                let body_text_result = tokio::select! {
                    biased;
                    _ = ctx.shutdown_notify.notified() => {
                        info!("Shutdown requested while reading poll error response body (4xx).");
                        return Err(LoungeError::ConnectionClosed);
                    }
                    text_res = response.text() => text_res,
                };
                let body_text = body_text_result.map_err(LoungeError::RequestFailed)?;
                error!(
                    "Terminal HTTP status ({}) from server during event poll; session likely dead. Body: {}",
                    status, body_text
                );
                return Ok(ConnectionStatus::SessionInvalidated);
            }
            401 => {
                warn!("Event poll received 401 Unauthorized.");
                return Ok(ConnectionStatus::TokenExpired); // Signal token expiry
            }
            status if !response.status().is_success() => {
                // Make text reading interruptible
                let body_text_result = tokio::select! {
                    biased;
                _ = ctx.shutdown_notify.notified() => {
                    info!("Shutdown requested while reading poll error response body (other).");
                    return Err(LoungeError::ConnectionClosed);
                    }
                text_res = response.text() => text_res,
                };
                let body_text = body_text_result.map_err(LoungeError::RequestFailed)?;

                error!(%status, body=%body_text, "Event poll received non-terminal unsuccessful status");
                return Err(LoungeError::InvalidResponse(format!(
                    "Polling error status {}, body: {}",
                    status, body_text
                )));
            }
            _ => {
                // Unexpected success codes?
                warn!(status=%response.status(), "Unexpected successful status code during event poll.");
                return Err(LoungeError::InvalidResponse(format!(
                    "Unexpected status {} during poll",
                    response.status()
                )));
            }
        } // End status match

        // --- Process Streaming Response Body ---
        // (The rest of the function with the select! around stream.next() remains the same)
        let mut stream = response.bytes_stream();
        let mut codec = LoungeCodec::new();
        let mut buffer = BytesMut::with_capacity(BUFFER_CAPACITY);
        let mut _received_data = false; // Keep track if we got any data in this poll cycle

        loop {
            // Use select! to race stream reading against shutdown notification
            tokio::select! {
                biased; // Check notification first

                    _ = ctx.shutdown_notify.notified() => {
                    info!("Shutdown requested during event polling.");
                    // Return a specific error or status to indicate graceful shutdown requested
                    return Err(LoungeError::ConnectionClosed); // Signal outer loop to stop
                }

                // Wait for the next chunk OR the inactivity timeout
                maybe_chunk_result = timeout(INACTIVITY_TIMEOUT, stream.next()) => {
                        match maybe_chunk_result {
                        // --- Case 1: Data received within timeout ---
                        Ok(Some(Ok(chunk))) => {
                            if chunk.is_empty() {
                                trace!("Received empty chunk in event stream.");
                                continue; // Ignore empty chunks, continue loop
                            }
                            _received_data = true;
                            trace!("Received chunk of size {}", chunk.len());
                            buffer.extend_from_slice(&chunk);
                            loop {
                                match codec.decode(&mut buffer) {
                                    Ok(Some(message)) => {
                                        trace!("Decoded message of size {}", message.len());
                                        events::process_event_chunk(
                                            &message, // Use ctx fields
                                            &ctx.event_sender,
                                            &ctx.latest_now_playing,
                                            &ctx.shared_state,
                                            &ctx.aid_atomic,
                                        )
                                        .await;
                                    }
                                    Ok(None) => {
                                        // Need more data in buffer to decode a full message
                                        trace!("Codec needs more data.");
                                        break; // Break inner loop, wait for more chunks in outer select!
                                    }
                                    Err(e) => {
                                        error!(error = %e, "Error decoding event message stream chunk");
                                        return Err(LoungeError::IoError(e)); // Fatal decoding error for this poll
                                    }
                                }
                            }
                        }

                        // --- Case 2: Stream returned an error within timeout ---
                        Ok(Some(Err(e))) => {
                            // Check if the error *or its source* is a timeout, especially for Body errors
                            use std::error::Error as StdError; // Alias trait
                            let is_body_timeout = e.is_body()
                                && e.source()
                                    .and_then(|source| {
                                        source
                                            .downcast_ref::<std::io::Error>()
                                            .map(|io_err| io_err.kind() == std::io::ErrorKind::TimedOut)
                                    })
                                    .unwrap_or(false);

                            if e.is_timeout() || is_body_timeout {
                                warn!(
                                    err = %e,
                                    "Timeout detected during stream read (reqwest internal or Body->TimedOut). Treating as Success and re-polling."
                                );
                                // Treat this specific timeout as a successful poll cycle end, prompting an immediate reconnect.
                                return Ok(ConnectionStatus::Success);
                            } else {
                                    // It's a different kind of network or decoding error. Log details.
                                    error!(
                                        err = %e,
                                        cause = ?e.source(),
                                        "Unhandled network/decode error during event stream chunk read. Triggering backoff."
                                    );
                                    // Treat other errors as failures needing backoff.
                                    return Err(LoungeError::RequestFailed(e));
                            }
                        }

                        // --- Case 3: Stream ended gracefully within timeout ---
                        Ok(None) => {
                            debug!("Event stream ended gracefully by server (EOF). Re-polling.");
                            return Ok(ConnectionStatus::Success); // End of this poll cycle
                        }

                        // --- Case 4: Inactivity Timeout expired ---
                        Err(_) => {
                            debug!(
                                "Inactivity detected (no data for >{}s), closing poll cycle. Re-polling.",
                                INACTIVITY_TIMEOUT.as_secs()
                            );
                                // Treat timeout like a graceful close, immediately try polling again
                                return Ok(ConnectionStatus::Success);
                        }
                    } // end maybe_chunk_result match
                } // end maybe_chunk_result branch
            } // end select!
        }
        // Note: Unreachable, loop should only be exited via returns above.
    }

    /// Helper function to attempt token refresh and update shared state.
    async fn try_refresh_token(
        screen_id: &str,
        shared_state: &Arc<RwLock<InnerState>>,
    ) -> Result<(), LoungeError> {
        match LoungeClient::refresh_lounge_token(screen_id).await {
            Ok(screen) => {
                info!("Successfully refreshed token for screen_id: {}", screen_id);
                let mut state = shared_state.write().await;
                let old_token_preview = state.lounge_token.chars().take(8).collect::<String>();
                state.lounge_token = screen.lounge_token.clone();
                debug!(old = %old_token_preview, "Stored new lounge token in shared state.");
                if let Some(ref callback) = state.token_refresh_callback {
                    debug!("Calling token refresh callback.");
                    callback(screen_id, &screen.lounge_token);
                } else {
                    debug!("No token refresh callback set.");
                }
                Ok(())
            }
            Err(refresh_err) => {
                error!(error = %refresh_err, "Failed to refresh token");
                Err(LoungeError::TokenRefreshFailed(Box::new(refresh_err)))
            }
        }
    }

    /// Send a playback command to the screen
    pub async fn send_command(&self, command: PlaybackCommand) -> Result<(), LoungeError> {
        // Check connection state first
        let current_state = self.current_state();
        if current_state != ConnectionState::Connected {
            warn!(state=?current_state, "Attempted to send command while not connected.");
            return Err(LoungeError::SessionLost);
        }

        let sid: String;
        let gsessionid: String;
        let rid_val: u32;
        let ofs_val: u32;
        let rid_string: String;
        let ofs_string: String;

        let token: String;

        {
            let session = self.session_state.read().await;
            // These unwraps are now safe due to the ConnectionState::Connected check above
            sid = session.sid.clone().ok_or(LoungeError::SessionLost)?;
            gsessionid = session.gsessionid.clone().ok_or(LoungeError::SessionLost)?;

            rid_val = session.rid.fetch_add(1, Ordering::SeqCst);
            ofs_val = session.command_offset.fetch_add(1, Ordering::SeqCst);
            rid_string = rid_val.to_string();
            ofs_string = ofs_val.to_string();
        }; // Release read lock on session_state

        {
            let state_guard = self.shared_state.read().await;
            token = state_guard.lounge_token.clone();
        }; // Release read lock on shared_state (token)

        let current_aid = self.aid_atomic.load(Ordering::SeqCst);
        let aid_string: String = current_aid.to_string();

        let command_name = command.name();
        debug!(
            "Sending command: {} (RID: {}, offset: {})",
            command_name, rid_val, ofs_val
        );

        let mut form_fields: Vec<(&str, String)> = Vec::with_capacity(16);
        form_fields.push(("count", "1".to_string()));
        form_fields.push(("ofs", ofs_string));
        form_fields.push(("req0__sc", command_name.to_string()));

        match &command {
            PlaybackCommand::SetPlaylist {
                video_id,
                list_id,
                current_index,
                current_time,
                audio_only,
                params,
                player_params,
            } => {
                form_fields.push(("req0_videoId", video_id.clone()));
                if let Some(idx) = current_index {
                    form_fields.push(("req0_currentIndex", idx.to_string()));
                }
                if let Some(list) = list_id {
                    form_fields.push(("req0_listId", list.clone()));
                }
                if let Some(time) = current_time {
                    form_fields.push(("req0_currentTime", time.to_string()));
                }
                if let Some(audio) = audio_only {
                    form_fields.push(("req0_audioOnly", audio.to_string()));
                }
                if let Some(p) = params {
                    form_fields.push(("req0_params", p.clone()));
                }
                if let Some(pp) = player_params {
                    form_fields.push(("req0_playerParams", pp.clone()));
                }
                form_fields.push((
                    "req0_prioritizeMobileSenderPlaybackStateOnConnection",
                    "true".to_string(),
                ));
            }
            PlaybackCommand::AddVideo {
                video_id,
                video_sources,
            } => {
                form_fields.push(("req0_videoId", video_id.clone()));
                if let Some(sources) = video_sources {
                    form_fields.push(("req0_videoSources", sources.clone()));
                }
            }
            PlaybackCommand::SeekTo { new_time } => {
                form_fields.push(("req0_newTime", new_time.to_string()));
            }
            PlaybackCommand::SetVolume { volume } => {
                form_fields.push(("req0_volume", volume.to_string()));
            }
            PlaybackCommand::SetAutoplayMode { autoplay_mode } => {
                form_fields.push(("req0_autoplayMode", autoplay_mode.clone()));
            }
            _ => {}
        }

        let params = [
            ("SID", sid.as_str()),
            ("gsessionid", gsessionid.as_str()),
            ("RID", rid_string.as_str()),
            ("VER", "8"),
            ("v", "2"),
            ("TYPE", "bind"),
            ("t", "1"),
            ("AID", aid_string.as_str()),
            ("CI", "0"),
            ("name", self.device_name.as_str()),
            ("id", self.device_id.as_str()),
            ("device", "REMOTE_CONTROL"),
            ("loungeIdToken", token.as_str()),
        ];

        debug!(?params, ?form_fields, "Sending command request");

        let response = self
            .client
            .post("https://www.youtube.com/api/lounge/bc/bind")
            .query(&params)
            .form(&form_fields)
            .send()
            .await
            .map_err(LoungeError::RequestFailed)?; // Map send error

        match response.status().as_u16() {
            200 => {
                debug!("Command sent successfully: {}", command_name);
                Ok(())
            }
            400 => {
                warn!(
                    "Session likely expired (HTTP 400) sending command: {}",
                    command_name
                );
                Err(LoungeError::SessionInvalidatedByServer(400))
            }
            401 => {
                warn!("Token expired (HTTP 401) sending command: {}", command_name);
                Err(LoungeError::TokenExpired)
            }
            404 => {
                warn!(
                    "Session not found (HTTP 404) sending command: {}",
                    command_name
                );
                Err(LoungeError::SessionInvalidatedByServer(404))
            }
            410 => {
                warn!(
                    "Connection closed (HTTP 410) sending command: {}",
                    command_name
                );
                Err(LoungeError::ConnectionClosed) // Or SessionInvalidated? ConnectionClosed seems slightly better.
            }
            status if !response.status().is_success() => {
                let body_text = response.text().await.map_err(LoungeError::RequestFailed)?;
                let error_msg = format!(
                    "Command '{}' failed with status {} and response body:\n{}",
                    command_name, status, body_text
                );
                error!("{}", error_msg);
                Err(LoungeError::InvalidResponse(error_msg))
            }
            _ => {
                warn!(status=%response.status(), "Unexpected successful status code sending command.");
                Err(LoungeError::InvalidResponse(format!(
                    "Unexpected status {} sending command",
                    response.status()
                )))
            }
        }
    }

    pub async fn send_command_with_refresh(
        &self,
        command: PlaybackCommand,
    ) -> Result<(), LoungeError> {
        match self.send_command(command.clone()).await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                info!(
                    "Refreshing expired token (send_command_with_refresh for '{}')",
                    command.name()
                );
                Self::try_refresh_token(&self.screen_id, &self.shared_state).await?;
                debug!(
                    "Retrying send_command for '{}' after refresh",
                    command.name()
                );
                // Need to check state *again* after refresh before retrying command
                if self.current_state() == ConnectionState::Connected {
                    self.send_command(command).await
                } else {
                    warn!("State is not Connected after token refresh, command aborted.");
                    Err(LoungeError::SessionLost) // Session might have been lost during refresh
                }
            }
            Err(e @ LoungeError::SessionInvalidatedByServer(_))
            | Err(e @ LoungeError::SessionLost) => {
                warn!("Command failed because session is invalid/lost: {}", e);
                Err(e) // Don't retry if session is known dead
            }
            Err(e) => Err(e),
        }
    }

    // Helper to stop and await the manager task
    async fn stop_and_await_manager(&self) -> Result<(), LoungeError> {
        let was_set = !self.stop_signal.swap(true, Ordering::SeqCst); // Use swap to check if already set
        self.shutdown_notify.notify_one(); // Notify any waiters
        debug!("Stop signal sent and notification triggered for manager task.");

        let handle = {
            let mut task_guard = self.management_task.write().await;
            task_guard.take() // Take the handle out of the Option
        };

        if let Some(h) = handle {
            if was_set {
                // Await only if we were the first to signal stop *now*
                debug!("Awaiting management task termination...");
                h.await.map_err(LoungeError::TaskJoinError)?; // Map JoinError
                debug!("Management task joined.");
            } else {
                debug!("Management task was already stopping or handle taken elsewhere.");
            }
        } else if was_set {
            // Only warn if we signalled stop but found no handle
            warn!("No management task handle found to await. Was connect called successfully?");
        }
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), LoungeError> {
        info!("Disconnecting from screen: {}", self.screen_id);

        // 1. Signal the connection manager task to stop & await it
        self.stop_and_await_manager().await?; // Await completion before proceeding

        // State should be Stopping or Disconnected now due to await/signal
        let _ = self
            .connection_state_tx
            .send_replace(ConnectionState::Stopping); // Ensure state reflects intention

        // 2. Read current session details FOR the terminate request
        let sid: Option<String>;
        let gsessionid: Option<String>;
        let rid_val: u32;
        let rid_string: String;
        // Token is needed for the terminate request parameters
        let token: String;

        {
            let session = self.session_state.read().await;
            sid = session.sid.clone();
            gsessionid = session.gsessionid.clone();
            rid_val = session.rid.fetch_add(1, Ordering::SeqCst);
            rid_string = rid_val.to_string();
        }

        {
            let state_guard = self.shared_state.read().await;
            token = state_guard.lounge_token.clone();
        }

        // 3. Send terminate request (best effort) if session existed
        if let (Some(sid_val), Some(gsessionid_val)) = (sid, gsessionid) {
            // Re-checked parameters based on earlier fix for 411 error
            let params = [
                ("RID", rid_string.as_str()), // Use incremented RID from session state
                ("VER", "8"),
                ("CVER", "1"),
                ("gsessionid", gsessionid_val.as_str()), // Session ID from session state
                ("SID", sid_val.as_str()),               // Other Session ID from session state
                ("auth_failure_option", "send_error"),
                ("name", self.device_name.as_str()),
                ("id", self.device_id.as_str()),
                ("device", "REMOTE_CONTROL"),
                ("loungeIdToken", token.as_str()), // Added token back, potentially needed
            ];

            let body_data = "ui=&TYPE=terminate&clientDisconnectReason=MDX_SESSION_DISCONNECT_REASON_DISCONNECTED_BY_USER";

            debug!(?params, "Sending disconnect (terminate) request");
            let res = self
                .client
                .post("https://www.youtube.com/api/lounge/bc/bind")
                .query(&params)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body_data)
                .timeout(Duration::from_secs(5))
                .send()
                .await;

            match res {
                Ok(response) if response.status().is_success() => {
                    debug!("Terminate request successful.");
                }
                Ok(response) => {
                    warn!(status=%response.status(), "Terminate request failed (status)");
                }
                Err(e) => {
                    warn!("Error sending terminate request (ignored): {}", e);
                }
            }
        } else {
            warn!("No valid session details found when disconnecting, cannot send explicit terminate request.");
        }

        // 4. Clear local session state AFTER attempting terminate and awaiting manager
        {
            let mut session_write = self.session_state.write().await;
            if session_write.sid.is_some() || session_write.gsessionid.is_some() {
                debug!("Clearing shared SessionState due to disconnect.");
                *session_write = SessionState::new();
            }
        }

        // 5. Send disconnect event and set final state
        let _ = self.event_sender.send(LoungeEvent::ScreenDisconnected);
        let _ = self
            .connection_state_tx
            .send_replace(ConnectionState::Disconnected);

        info!("Client disconnected.");
        Ok(())
    }

    /// Builds the form data needed for the initial bind request.
    async fn build_connect_form_data(&self) -> Result<String, LoungeError> {
        let token = {
            let state_guard = self.shared_state.read().await;
            state_guard.lounge_token.clone()
        };
        let form_fields: Vec<(&str, &str)> = vec![
            ("app", "youtube-desktop"),
            ("mdx-version", "3"),
            ("name", &self.device_name),
            ("id", &self.device_id),
            ("device", "REMOTE_CONTROL"),
            ("capabilities", "que,dsdtr,atp"),
            ("magnaKey", "cloudPairedDevice"),
            ("ui", "false"),
            ("theme", "cl"),
            ("loungeIdToken", &token),
        ];

        serde_urlencoded::to_string(&form_fields).map_err(LoungeError::UrlEncodingFailed)
    }

    pub fn get_thumbnail_url(video_id: &str, thumbnail_idx: u8) -> String {
        format!(
            "https://img.youtube.com/vi/{}/{}.jpg",
            video_id, thumbnail_idx
        )
    }

    // --- Command Wrappers ---

    pub async fn play(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Play).await
    }

    pub async fn pause(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Pause).await
    }

    pub async fn next(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Next).await
    }

    pub async fn previous(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Previous)
            .await
    }

    pub async fn skip_ad(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SkipAd)
            .await
    }

    pub async fn mute(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Mute).await
    }

    pub async fn unmute(&self) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::Unmute)
            .await
    }

    pub async fn seek_to(&self, new_time: f64) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SeekTo { new_time })
            .await
    }

    pub async fn set_volume(&self, volume: i32) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SetVolume { volume })
            .await
    }

    pub async fn set_autoplay_mode(&self, autoplay_mode: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::SetAutoplayMode { autoplay_mode })
            .await
    }

    pub async fn play_video(&self, video_id: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::set_playlist(video_id))
            .await
    }

    pub async fn add_video_to_queue(&self, video_id: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::add_video(video_id))
            .await
    }

    pub async fn play_playlist(&self, list_id: String) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::set_playlist_by_id(list_id))
            .await
    }

    pub async fn play_playlist_at_index(
        &self,
        list_id: String,
        index: i32,
    ) -> Result<(), LoungeError> {
        self.send_command_with_refresh(PlaybackCommand::set_playlist_with_index(list_id, index))
            .await
    }
}

// Safety traits
impl std::fmt::Debug for LoungeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid reading session state here as it requires await
        f.debug_struct("LoungeClient")
            .field("device_id", &self.device_id)
            .field("screen_id", &self.screen_id)
            .field("device_name", &self.device_name)
            .finish()
    }
}

// Ensure the client cleans up the background task on drop
impl Drop for LoungeClient {
    fn drop(&mut self) {
        info!("Dropping LoungeClient, signaling connection manager to stop.");
        // Signal the background task to stop, don't await here as drop shouldn't block
        self.stop_signal.store(true, Ordering::SeqCst);
        // We don't explicitly wait for the task to finish here,
        // but the signal should cause it to break its loop.
    }
}

/// Helper to calculate backoff delay with jitter
fn calculate_backoff_delay(base_backoff: Duration) -> Duration {
    let jitter_factor = rand::random::<f32>() * 0.6 - 0.3; // -0.3 to +0.3
    let jitter = base_backoff.mul_f32(jitter_factor.abs());
    let delay = if jitter_factor >= 0.0 {
        base_backoff + jitter
    } else {
        base_backoff - jitter
    };
    delay.max(Duration::ZERO) // Ensure non-negative
}
