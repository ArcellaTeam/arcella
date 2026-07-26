// arcella/arcella-broker/src/client/mod.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;

pub mod registry;
pub mod in_memory;

use crate::protocol::Message;
use crate::transport::{Transport, TransportError, TransportResult};
use registry::LocalRegistry;
use in_memory::InMemoryTransport;

pub struct BrokerClient {
    registry: Arc<LocalRegistry>,
    local: InMemoryTransport,
    // remote: Option<IpcTransport>,  // Will be added in stage 2
}

impl BrokerClient {
    pub fn new(registry: Arc<LocalRegistry>) -> Self {
        let (local, _incoming_tx) = InMemoryTransport::new(registry.clone());
        Self { registry, local }
    }
    
    /// Register itself as a receiver at the specified address.
    pub async fn bind(&self, address: String, incoming_tx: tokio::sync::mpsc::Sender<Message>) {
        self.registry.register(address, incoming_tx).await;
    }

    /// Send a message (InOnly).
    pub async fn send(&self, address: &str, message: Message) -> TransportResult<()> {
        // Priority 1: local delivery
        if self.registry.has_local(address).await {
            return self.local.send(address, message).await;
        }
        
        // Priority 2: IPC (stage 2)
        // self.remote.as_ref().ok_or(TransportError::RecipientNotFound(...))?
        //     .send(address, message).await
        
        Err(TransportError::RecipientNotFound(address.to_string()))
    }

    /// Send a request and wait for a response (InOut).
    pub async fn request(&self, address: &str, message: Message) -> TransportResult<Message> {
        if self.registry.has_local(address).await {
            return self.local.request(address, message).await;
        }
        
        Err(TransportError::RecipientNotFound(address.to_string()))
    }

}