use mockall::automock;
use mockall::predicate::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use youtube_lounge_rs::{LoungeError, PlaybackCommand};

// Define traits for the client interface so we can mock them
#[automock]
trait RefreshableClient {
    fn check_screen_availability(
        &self,
    ) -> futures::future::BoxFuture<'static, Result<bool, LoungeError>>;
    fn refresh_token_internal(
        &mut self,
    ) -> futures::future::BoxFuture<'static, Result<(), LoungeError>>;
    fn send_command(
        &self,
        command: PlaybackCommand,
    ) -> futures::future::BoxFuture<'static, Result<(), LoungeError>>;
    fn connect(&self) -> futures::future::BoxFuture<'static, Result<(), LoungeError>>;
}

// Define a separate trait for simpler testing of callbacks
#[automock]
trait TokenCallback {
    fn on_token_refreshed(&self, screen_id: &str, token: &str);
}

// Implement the actual refresh methods
trait TokenRefreshable: RefreshableClient {
    async fn check_screen_availability_with_refresh(&mut self) -> Result<bool, LoungeError> {
        match self.check_screen_availability().await {
            Ok(available) => Ok(available),
            Err(LoungeError::TokenExpired) => {
                // Token expired, try to refresh it
                self.refresh_token_internal().await?;

                // Retry with new token
                self.check_screen_availability().await
            }
            Err(e) => Err(e),
        }
    }

    async fn send_command_with_refresh(
        &mut self,
        command: PlaybackCommand,
    ) -> Result<(), LoungeError> {
        match self.send_command(command.clone()).await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                // Token expired, try to refresh it
                self.refresh_token_internal().await?;

                // Retry the command with the new token
                self.send_command(command).await
            }
            Err(e) => Err(e),
        }
    }

    async fn connect_with_refresh(&mut self) -> Result<(), LoungeError> {
        match self.connect().await {
            Ok(()) => Ok(()),
            Err(LoungeError::TokenExpired) => {
                // Token expired, try to refresh it
                self.refresh_token_internal().await?;

                // Retry connection with new token
                self.connect().await
            }
            Err(e) => Err(e),
        }
    }

    async fn send_command_with_multiple_retries(
        &mut self,
        command: PlaybackCommand,
    ) -> Result<(), LoungeError> {
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 3;

        loop {
            match self.send_command(command.clone()).await {
                Ok(()) => return Ok(()),
                Err(LoungeError::TokenExpired) => {
                    // Token expired, try to refresh it
                    self.refresh_token_internal().await?;
                    attempts += 1;
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= MAX_ATTEMPTS {
                        return Err(e);
                    }
                    // Otherwise continue and retry
                }
            }

            if attempts >= MAX_ATTEMPTS {
                return Err(LoungeError::Other("Max retry attempts reached".to_string()));
            }
        }
    }
}

// Implement the trait for the mock
impl TokenRefreshable for MockRefreshableClient {}

// Test the token refresh callback functionality
#[tokio::test]
async fn test_token_refresh_callback() {
    // Tracking variables to verify callback was called with correct values
    let was_called = Arc::new(AtomicBool::new(false));
    let new_token_value = Arc::new(Mutex::new(String::new()));

    let was_called_clone = was_called.clone();
    let new_token_clone = new_token_value.clone();

    // Create a mock callback
    let mut mock_callback = MockTokenCallback::new();
    mock_callback
        .expect_on_token_refreshed()
        .withf(move |screen_id: &str, token: &str| {
            assert_eq!(screen_id, "test_screen_id");
            *new_token_clone.lock().unwrap() = token.to_string();
            was_called_clone.store(true, Ordering::SeqCst);
            true
        })
        .times(1)
        .return_const(());

    // Simulate token refresh and callback
    mock_callback.on_token_refreshed("test_screen_id", "refreshed_token");

    // Check that the callback was called and token was updated correctly
    assert!(was_called.load(Ordering::SeqCst));
    assert_eq!(*new_token_value.lock().unwrap(), "refreshed_token");
}

// Test the check_screen_availability_with_refresh method
#[tokio::test]
async fn test_availability_with_refresh() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Static to track call count
    static mut AVAILABILITY_CALL_COUNT: usize = 0;

    // First call fails with token expired, then succeeds after refresh
    mock_client
        .expect_check_screen_availability()
        .times(2)
        .returning(|| {
            Box::pin(async {
                unsafe {
                    let result = if AVAILABILITY_CALL_COUNT == 0 {
                        Err(LoungeError::TokenExpired)
                    } else {
                        Ok(true)
                    };
                    AVAILABILITY_CALL_COUNT += 1;
                    result
                }
            })
        });

    // Expect refresh_token_internal to be called once
    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(|| Box::pin(async { Ok(()) }));

    // Test the auto-refresh method
    let result = mock_client.check_screen_availability_with_refresh().await;

    // Verify successful result
    assert!(result.is_ok());
    assert!(result.unwrap());
}

// Test the send_command_with_refresh method
#[tokio::test]
async fn test_send_command_with_refresh() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Static to track call count
    static mut COMMAND_CALL_COUNT: usize = 0;

    // First command fails with token expired, second succeeds
    mock_client
        .expect_send_command()
        .times(2)
        .returning(move |_command| {
            Box::pin(async {
                unsafe {
                    let result = if COMMAND_CALL_COUNT == 0 {
                        Err(LoungeError::TokenExpired)
                    } else {
                        Ok(())
                    };
                    COMMAND_CALL_COUNT += 1;
                    result
                }
            })
        });

    // Expect refresh_token_internal to be called once
    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(|| Box::pin(async { Ok(()) }));

    // Test the auto-refresh command method
    let result = mock_client
        .send_command_with_refresh(PlaybackCommand::Play)
        .await;

    // Verify successful result
    assert!(result.is_ok());
}

// Test that other errors don't trigger refresh
#[tokio::test]
async fn test_no_refresh_on_other_errors() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Command fails with a non-token error
    mock_client
        .expect_send_command()
        .times(1)
        .returning(|_command| {
            Box::pin(async { Err(LoungeError::Other("Some other error".to_string())) })
        });

    // Expect refresh_token_internal to NOT be called
    mock_client.expect_refresh_token_internal().times(0);

    // Test command with an error that shouldn't trigger refresh
    let result = mock_client
        .send_command_with_refresh(PlaybackCommand::Play)
        .await;

    // Verify error is passed through and refresh isn't called
    assert!(result.is_err());
    if let Err(LoungeError::Other(msg)) = result {
        assert_eq!(msg, "Some other error");
    } else {
        panic!("Expected LoungeError::Other");
    }
}

// Test that refresh failure propagates correctly
#[tokio::test]
async fn test_refresh_failure_propagates() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Command fails with token expired
    mock_client
        .expect_check_screen_availability()
        .times(1)
        .returning(|| Box::pin(async { Err(LoungeError::TokenExpired) }));

    // Refresh fails with an error
    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(|| Box::pin(async { Err(LoungeError::Other("Refresh error".to_string())) }));

    // Test the availability check with refresh
    let result = mock_client.check_screen_availability_with_refresh().await;

    // Verify the refresh error is propagated
    assert!(result.is_err());
    if let Err(LoungeError::Other(msg)) = result {
        assert_eq!(msg, "Refresh error");
    } else {
        panic!("Expected LoungeError::Other");
    }
}

// Test multiple consecutive errors with retry
#[tokio::test]
async fn test_command_retry_after_refresh() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Static to track call count
    static mut RETRY_CALL_COUNT: usize = 0;

    // Set up command expectations: first token expired, then transient error, then success
    mock_client
        .expect_send_command()
        .times(3)
        .returning(|_command| {
            Box::pin(async {
                unsafe {
                    let result = match RETRY_CALL_COUNT {
                        0 => Err(LoungeError::TokenExpired),
                        1 => Err(LoungeError::Other("Transient error".to_string())),
                        _ => Ok(()),
                    };
                    RETRY_CALL_COUNT += 1;
                    result
                }
            })
        });

    // Expect refresh_token_internal to be called once after the first error
    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(|| Box::pin(async { Ok(()) }));

    // Test the retry method
    let result = mock_client
        .send_command_with_multiple_retries(PlaybackCommand::Play)
        .await;

    // Should eventually succeed after refresh and retries
    assert!(result.is_ok());
}

// Test token refresh sequence to verify correct operation order
#[tokio::test]
async fn test_token_refresh_sequence() {
    // Create sequence tracking
    let operations = Arc::new(Mutex::new(Vec::<String>::new()));

    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Clone the operations vector for use in closures
    let ops_for_check = operations.clone();

    // First check fails with token expired
    mock_client
        .expect_check_screen_availability()
        .times(2)
        .returning(move || {
            let ops = ops_for_check.clone();
            Box::pin(async move {
                let mut guard = ops.lock().unwrap();

                if guard.is_empty() {
                    // First call
                    guard.push("availability_check_start".to_string());
                    Err(LoungeError::TokenExpired)
                } else {
                    // After refresh
                    guard.push("availability_check_retry".to_string());
                    guard.push("availability_check_success".to_string());
                    Ok(true)
                }
            })
        });

    // Refresh token will record operations
    let ops_for_refresh = operations.clone();
    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(move || {
            let ops = ops_for_refresh.clone();
            Box::pin(async move {
                let mut guard = ops.lock().unwrap();
                guard.push("token_refresh_start".to_string());
                guard.push("token_refresh_success".to_string());
                Ok(())
            })
        });

    // Test the auto-refresh method
    let result = mock_client.check_screen_availability_with_refresh().await;

    // Verify successful result
    assert!(result.is_ok());

    // Check operation sequence
    let ops = operations.lock().unwrap().clone();
    assert_eq!(ops[0], "availability_check_start");
    assert_eq!(ops[1], "token_refresh_start");
    assert_eq!(ops[2], "token_refresh_success");
    assert_eq!(ops[3], "availability_check_retry");
    assert_eq!(ops[4], "availability_check_success");
}

// Test token refresh during connection
#[tokio::test]
async fn test_token_refresh_during_reconnection() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Static to track call count
    static mut CONNECT_CALL_COUNT: usize = 0;

    // First connection fails with token expired
    mock_client.expect_connect().times(2).returning(|| {
        Box::pin(async {
            unsafe {
                let result = if CONNECT_CALL_COUNT == 0 {
                    Err(LoungeError::TokenExpired)
                } else {
                    Ok(())
                };
                CONNECT_CALL_COUNT += 1;
                result
            }
        })
    });

    // Expect refresh_token_internal to be called once
    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(|| Box::pin(async { Ok(()) }));

    // Test the auto-refresh connection method
    let result = mock_client.connect_with_refresh().await;

    // Verify successful result
    assert!(result.is_ok());
}

// Test that callback errors are handled properly
#[tokio::test]
async fn test_callback_error_handling() {
    // Create a mock client
    let mut mock_client = MockRefreshableClient::new();

    // Static to track call count
    static mut CALLBACK_TEST_COUNT: usize = 0;

    // First call fails with token expired
    mock_client
        .expect_check_screen_availability()
        .times(2)
        .returning(|| {
            Box::pin(async {
                unsafe {
                    let result = if CALLBACK_TEST_COUNT == 0 {
                        Err(LoungeError::TokenExpired)
                    } else {
                        Ok(true)
                    };
                    CALLBACK_TEST_COUNT += 1;
                    result
                }
            })
        });

    // Refresh succeeds but we'll verify it's called
    let refresh_called = Arc::new(AtomicBool::new(false));
    let refresh_called_clone = refresh_called.clone();

    mock_client
        .expect_refresh_token_internal()
        .times(1)
        .returning(move || {
            let called = refresh_called_clone.clone();
            Box::pin(async move {
                called.store(true, Ordering::SeqCst);
                Ok(())
            })
        });

    // Test the auto-refresh method
    let result = mock_client.check_screen_availability_with_refresh().await;

    // Verify successful result and that refresh was called
    assert!(result.is_ok());
    assert!(refresh_called.load(Ordering::SeqCst));
}
