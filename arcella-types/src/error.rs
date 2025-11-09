// arcella/arcella-types/src/error.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use thiserror::Error;

/// Result type alias for `arcella-wasmtime` operations.
pub type Result<T> = std::result::Result<T, ArcellaTypeError>;

#[derive(Error, Debug)]
pub enum ArcellaTypeError {
    
    /// Invalid or missing module manifest.
    #[error("Manifest error: {0}")]
    Manifest(String),

}