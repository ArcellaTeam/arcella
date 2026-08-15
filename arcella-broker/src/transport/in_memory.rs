// arcella/arcella-broker/src/transport/in_memory.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

use crate::protocol::Message;
use crate::registry::LocalRegistry;

use super::{Transport, TransportError, TransportResult};

/// Transport for in-process delivery.
/// 
/// Messages are routed directly through `tokio::mpsc` channels,
/// bypassing IPC. It is used when the recipient is located
/// in the same process as the sender.
pub struct InMemoryTransport {
    registry: Arc<LocalRegistry>,
}

impl InMemoryTransport {
    pub fn new(registry: Arc<LocalRegistry>) -> Self {
        Self { registry }
    }
}

impl Transport for InMemoryTransport {
    fn send<'a>(
        &'a self,
        address: &'a str,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = TransportResult<()>> + Send + 'a>> {
        Box::pin(async move {
            match self.registry.lookup(address) {
                Some(channel) => {
                    channel.send(message).await.map_err(|_| {
                        TransportError::RecipientNotFound(address.to_string())
                    })?;
                    Ok(())
                }
                None => Err(TransportError::RecipientNotFound(address.to_string())),
            }
        })
    }

    fn request<'a>(
        &'a self,
        address: &'a str,
        mut message: Message,
    ) -> Pin<Box<dyn Future<Output = TransportResult<Message>> + Send + 'a>> {
        Box::pin(async move {
            // TODO
            Err(TransportError::ConnectionClosed)
        })
    }

    fn receive<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = TransportResult<Message>> + Send + 'a>> {
        Box::pin(async move {
            // TODO
            Err(TransportError::ConnectionClosed)
        })
    }

    fn close<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = TransportResult<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}