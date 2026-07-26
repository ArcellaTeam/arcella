// arcella/arcella-broker/src/client/moregistry.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use crate::protocol::Message;

/// Channel type for local delivery
pub type LocalChannel = mpsc::Sender<Message>;

/// Registry of local recipients (within a single process).
/// 
/// Key — hierarchical address (e.g., "arcella:core:users").
/// Value — sender to the recipient's queue.
#[derive(Default)]
pub struct LocalRegistry {
    recipients: RwLock<HashMap<String, LocalChannel>>,
}

impl LocalRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Register a recipient at the specified address
    pub async fn register(&self, address: String, channel: LocalChannel) {
        self.recipients.write().await.insert(address, channel);
    }

    /// Unregister a recipient
    pub async fn unregister(&self, address: &str) {
        self.recipients.write().await.remove(address);
    }

    /// Find a local channel for the given address.
    /// Returns `Some(channel)` if the recipient exists in this process.
    pub async fn lookup(&self, address: &str) -> Option<LocalChannel> {
        self.recipients.read().await.get(address).cloned()
    }

    /// Check if a local recipient exists for the given address
    pub async fn has_local(&self, address: &str) -> bool {
        self.recipients.read().await.contains_key(address)
    }
}