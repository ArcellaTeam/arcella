// arcella/arcella-broker/src/client/registry.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use dashmap::DashMap;
use tokio::sync::mpsc;
use crate::protocol::Message;

/// Channel type for local delivery
pub type LocalChannel = mpsc::Sender<Message>;

/// Registry of local recipients (within a single process).
/// 
/// Key — hierarchical address (e.g., "arcella:core:users").
/// Value — sender to the recipient's queue.
#[derive(Default)]
pub struct LocalRegistry {
    recipients: DashMap<String, LocalChannel>,
}

impl LocalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a recipient at the specified address
    pub fn register(&self, address: String, channel: LocalChannel) {
        self.recipients.insert(address, channel);
    }

    /// Unregister a recipient
    pub fn unregister(&self, address: &str) {
        self.recipients.remove(address);
    }

    /// Find a local channel for the given address.
    /// Returns `Some(channel)` if the recipient exists in this process.
    pub fn lookup(&self, address: &str) -> Option<LocalChannel> {
        self.recipients.get(address).map(|entry| entry.value().clone())
    }

    /// Check if a local recipient exists for the given address
    pub fn has_local(&self, address: &str) -> bool {
        self.recipients.contains_key(address)
    }
}