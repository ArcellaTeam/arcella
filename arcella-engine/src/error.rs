// arcella/arcella-core/src/engine/error.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Centralized common error handling for Arcella Engine.

use std::path::PathBuf;
use thiserror::Error;

use arcella_types::ArcellaTypeError;

#[derive(Error, Debug)]
pub enum ArcellaEngineError {

    #[error("Arcella types error: {0}")]
    ArcellaTypeError (#[from] ArcellaTypeError),    

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Engine error: {0}")]
    EngineError(String),

    #[error("Component introspection error: {0}")]
    Introspection(String),

    /// I/O error (file not found, permission denied, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// IO error with associated path for better diagnostics
    #[error("I/O error at {path:?}: {source}")]
    IoWithPath {
        source: std::io::Error,
        path: PathBuf,
    },
    
    /// Invalid or missing module manifest.
    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("Memory too small: {0}")]
    MemoryTooLarge(u32),

    #[error("Memory too small: {0}")]
    MemoryTooSmall(u32),

}

/// Result type alias for `arcella-engine` operations.
pub type ArcellaEngineResult<T> = std::result::Result<T, ArcellaEngineError>;
