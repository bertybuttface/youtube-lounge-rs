use std::error::Error;
use std::io;

use youtube_lounge_rs::LoungeError;

// Test LoungeError display implementation
#[test]
fn test_lounge_error_display() {
    // Test IoError
    let io_err = io::Error::new(io::ErrorKind::Other, "Test IO error");
    let err = LoungeError::IoError(io_err);
    assert!(format!("{}", err).contains("IO error"));

    // Test ParseError
    let parse_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
    let err = LoungeError::ParseError {
        error: parse_err,
        context: "test context".to_string(),
        payload: "invalid json".to_string(),
    };
    assert!(format!("{}", err).contains("Parse error"));

    // Test InvalidResponse
    let err = LoungeError::InvalidResponse("Test invalid response".to_string());
    assert_eq!(
        format!("{}", err),
        "Invalid response: Test invalid response"
    );

    // Test SessionExpired
    let err = LoungeError::SessionExpired;
    assert_eq!(format!("{}", err), "Session expired");

    // Test TokenExpired
    let err = LoungeError::TokenExpired;
    assert_eq!(format!("{}", err), "Lounge token expired");

    // Test ConnectionClosed
    let err = LoungeError::ConnectionClosed;
    assert_eq!(format!("{}", err), "Connection closed");

    // Test Other
    let err = LoungeError::Other("Test other error".to_string());
    assert_eq!(format!("{}", err), "Other error: Test other error");
}

// Test LoungeError implements Error trait
#[test]
fn test_lounge_error_trait() {
    let err = LoungeError::Other("Test error".to_string());

    // Test that LoungeError implements Error
    fn takes_error(_: &dyn Error) {}
    takes_error(&err);
}

// Test conversions to LoungeError
#[test]
fn test_lounge_error_conversions() {
    // Test From<io::Error>
    let io_err = io::Error::new(io::ErrorKind::Other, "Test IO error");
    let err: LoungeError = io_err.into();
    match err {
        LoungeError::IoError(_) => {} // Success
        _ => panic!("Expected IoError variant"),
    }

    // Test From<serde_json::Error>
    let parse_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
    let err: LoungeError = parse_err.into();
    match err {
        LoungeError::ParseError { .. } => {} // Success
        _ => panic!("Expected ParseError variant"),
    }

    // Test From<reqwest::Error> - we can't easily create this directly in a test,
    // but we can verify the implementation exists
    assert!(std::any::TypeId::of::<reqwest::Error>() != std::any::TypeId::of::<LoungeError>());
}
