use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use crate::TokenCallback;

pub(crate) struct InnerState {
    pub(crate) lounge_token: String,
    pub(crate) token_refresh_callback: TokenCallback,
}

#[derive(Clone)]
pub(crate) struct SessionState {
    pub(crate) sid: Option<String>,
    pub(crate) gsessionid: Option<String>,
    pub(crate) rid: Arc<AtomicU32>,
    pub(crate) command_offset: Arc<AtomicU32>,
}

impl SessionState {
    pub(crate) fn new() -> Self {
        Self {
            sid: None,
            gsessionid: None,
            rid: Arc::new(AtomicU32::new(1)),
            command_offset: Arc::new(AtomicU32::new(0)),
        }
    }

    pub(crate) fn increment_rid(&self) -> u32 {
        self.rid.fetch_add(1, Ordering::SeqCst)
    }
    pub(crate) fn increment_offset(&self) -> u32 {
        self.command_offset.fetch_add(1, Ordering::SeqCst)
    }
}
