// arcella/arcella-types/src/error.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ArcellaError {
    
    /// Invalid or missing module manifest.
    #[error("Manifest error: {0}")]
    ManifestError(String),

    /// Invalid module ID format.
    #[error("Invalid module ID format: {0}")]
    InvalidModuleIdFormat(String),

    /// Invalid module ID namespace.
    #[error("Invalid module ID name: {0}")]
    InvalidModuleIdName(String),

    /// Invalid module ID version.
    #[error("Invalid module ID version: {0}")]
    InvalidModuleIdVersion(String),

}

/// Result type alias
pub type ArcellaResult<T> = std::result::Result<T, ArcellaError>;
