// arcella/arcella-broker/src/test_utils.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use crate::client::registry::LocalRegistry;
use crate::client::BrokerClient;
use std::sync::Arc;

/// Create a test client with a local reestr
pub fn test_client() -> BrokerClient {
    let registry = LocalRegistry::new();
    BrokerClient::new(registry)
}

/// Create a test client with a shared reestr (for multiple clients)
pub fn test_client_with_shared_registry(registry: Arc<LocalRegistry>) -> BrokerClient {
    BrokerClient::new(registry)
}