// arcella/arcella-broker/src/client/in_memory.rs
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
use tokio::sync::mpsc;
use crate::protocol::Message;
use crate::transport::{Transport, TransportError, TransportResult};
use super::registry::LocalRegistry;

/// Transport for in-process delivery.
/// 
/// Messages are routed directly through `tokio::mpsc` channels,
/// bypassing IPC. It is used when the recipient is located
/// in the same process as the sender.
pub struct InMemoryTransport {
    registry: Arc<LocalRegistry>,
    /// Channel for incoming messages (if this client is also a recipient)
    incoming_rx: Mutex<mpsc::Receiver<Message>>,
}

use tokio::sync::Mutex;

impl InMemoryTransport {
    pub fn new(registry: Arc<LocalRegistry>) -> (Self, mpsc::Sender<Message>) {
        let (tx, rx) = mpsc::channel(1024);
        (
            Self {
                registry,
                incoming_rx: Mutex::new(rx),
            },
            tx,
        )
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
            use crate::protocol::TransferMode;
            message.header.flags = TransferMode::InOut.to_flags();
            
            // For InOut in in-memory mode, we use a temporary reply channel
            let (reply_tx, mut reply_rx) = mpsc::channel::<Message>(1);
            
            // In a real scenario, reply_tx needs to be injected into the message
            // so the recipient can send a response. Simplified for now:
            self.send(address, message).await?;
            
            reply_rx.recv().await.ok_or(TransportError::ConnectionClosed)
        })
    }

    fn receive<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = TransportResult<Message>> + Send + 'a>> {
        Box::pin(async move {
            let mut rx = self.incoming_rx.lock().await;
            rx.recv().await.ok_or(TransportError::ConnectionClosed)
        })
    }

    fn close<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = TransportResult<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}