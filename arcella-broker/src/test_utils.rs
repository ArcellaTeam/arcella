// arcella/arcella-broker/src/test_utils.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use bytes::Bytes;
use std::sync::Arc;

use crate::client::BrokerClient;
use crate::protocol::{Message, TransferMode};
use crate::registry::LocalRegistry;

/// Create a test client with a local reestr
pub fn test_client() -> BrokerClient {
    let registry = Arc::new(LocalRegistry::new());
    BrokerClient::new(registry)
}

/// Create a test client with a shared reestr (for multiple clients)
pub fn test_client_with_shared_registry(registry: Arc<LocalRegistry>) -> BrokerClient {
    BrokerClient::new(registry)
}

pub fn dummy_in_only_message(msg_type: Bytes, address: Bytes, payload: Bytes) -> Message {
    Message::new(
        TransferMode::InOnly,
        [0u8; 32],
        [1u8; 16],
        [0u8; 4],
        64,
        msg_type,
        address,
        payload,
    ).unwrap()
}