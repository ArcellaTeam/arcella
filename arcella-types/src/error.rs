// arcella/arcella-types/src/error.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;
use thiserror::Error;

/// Result type alias for `arcella-wasmtime` operations.
pub type Result<T> = std::result::Result<T, ArcellaTypeError>;

#[derive(Error, Debug)]
pub enum ArcellaTypeError {
    
    /// Invalid or missing module manifest.
    #[error("Manifest error: {0}")]
    Manifest(String),

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArcellaModuleIdError {
    InvalidFormat(String),
    InvalidName(String),
    InvalidVersion(String),
}

impl fmt::Display for ArcellaModuleIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArcellaModuleIdError::InvalidFormat(s) => write!(f, "Invalid module ID format: '{}'. Expected 'name@1.2.3'", s),
            ArcellaModuleIdError::InvalidName(s) => write!(f, "Invalid module name: '{}'. Must be alphanumeric with '-' or '_'", s),
            ArcellaModuleIdError::InvalidVersion(s) => write!(f, "Invalid version: '{}'. Must be 'x.y.z' with no leading zeros", s),
        }
    }
}

impl std::error::Error for ArcellaModuleIdError {}
