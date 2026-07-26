// arcella/arcella-broker/src/error.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use thiserror::Error;

use crate::protocol::ProtocolError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BrokerError {
    #[error("Arcella broker protocol error: {0}")]
    ProtocolError (#[from] ProtocolError),
}