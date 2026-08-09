// arcella/arcella-broker/src/client/subscriber.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::protocol::Message;
use super::registry::LocalRegistry;

pub struct Subscriber {
    rx: mpsc::Receiver<Message>,
    address: String,
    registry: Arc<LocalRegistry>,
}

impl Subscriber {
    pub(crate) fn new(
        rx: mpsc::Receiver<Message>,
        address: String,
        registry: Arc<LocalRegistry>,
    ) -> Self {
        Self { rx, address, registry }
    }

    pub async fn recv(&mut self) -> Option<Message> {
        self.rx.recv().await
    }

    pub fn try_recv(&mut self) -> Result<Message, tokio::sync::mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }  

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn unsubscribe(self) {
        drop(self);
    }        

}

impl Drop for Subscriber {
    fn drop(&mut self) {
        // Автоматическая очистка при выходе из области видимости
        self.registry.unregister(&self.address);
    }
}