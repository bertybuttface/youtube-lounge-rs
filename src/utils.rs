use lazy_static::lazy_static;
use regex::Regex;

use crate::LoungeError;

// Helper module for parsing YouTube's string values
pub mod youtube_parse {
    #[allow(dead_code)]
    pub fn parse_float(s: &str) -> f64 {
        s.parse::<f64>().unwrap_or(0.0)
    }

    pub fn parse_int(s: &str) -> i32 {
        s.parse::<i32>().unwrap_or(0)
    }

    pub fn parse_bool(s: &str) -> bool {
        s == "true"
    }

    pub fn parse_list(s: &str) -> Vec<String> {
        s.split(',').map(|s| s.trim().to_string()).collect()
    }
}

lazy_static! {
    static ref SID_RE: Regex = Regex::new(r#"\["c","([^"]*)""#).unwrap();
    static ref GSESSIONID_RE: Regex = Regex::new(r#"\["S","([^"]*)""#).unwrap();
}
pub fn extract_session_ids(body: &[u8]) -> Result<(Option<String>, Option<String>), LoungeError> {
    let full_response = String::from_utf8_lossy(body);
    let sid = SID_RE
        .captures(&full_response)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()));
    let gsessionid = GSESSIONID_RE
        .captures(&full_response)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()));
    match (sid, gsessionid) {
        (Some(sid), Some(gsessionid)) => Ok((Some(sid), Some(gsessionid))),
        _ => Err(LoungeError::InvalidResponse(
            "Failed to obtain session IDs".to_string(),
        )),
    }
}
