// arcella/arcella-broker/src/transport/mod.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::future::Future;
use std::pin::Pin;

use crate::protocol::Message;

/// Transport error
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Operation timeout")]
    Timeout,
    
    #[error("Recipient not found: {0}")]
    RecipientNotFound(String),
    
    #[error("Protocol error: {0}")]
    Protocol(#[from] crate::protocol::ProtocolError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type TransportResult<T> = Result<T, TransportError>;

/// Abstract transport for sending and receiving messages.
pub trait Transport: Send + Sync {
    /// Send a message to a recipient at the specified address.
    fn send<'a>(
        &'a self,
        address: &'a str,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = TransportResult<()>> + Send + 'a>>;

    /// Send a request and wait for a response (InOut mode).
    fn request<'a>(
        &'a self,
        address: &'a str,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = TransportResult<Message>> + Send + 'a>>;

    /// Receive the next incoming message (used on the server side).
    fn receive<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = TransportResult<Message>> + Send + 'a>>;

    /// Close the transport connection.
    fn close<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = TransportResult<()>> + Send + 'a>>;
}

