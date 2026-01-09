// arcella/arcella-wasmtime/src/error.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::PathBuf;
use thiserror::Error;

use arcella_types::ArcellaTypeError;

/// Result type alias for `arcella-wasmtime` operations.
pub type ArcellaWasmtimeResult<T> = std::result::Result<T, ArcellaWasmtimeError>;

/// Errors that can occur during Wasmtime-to-Arcella conversion.
#[derive(Error, Debug)]
enum ArcellaWasmtimeError {

    /// I/O error (file not found, permission denied, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// IO error with associated path for better diagnostics
    #[error("I/O error at {path:?}: {source}")]
    IoWithPath {
        source: std::io::Error,
        path: PathBuf,
    },

    #[error("Arcella types error: {0}")]
    ArcellaTypeError (#[from] ArcellaTypeError),    
    
    /// Invalid or missing module manifest.
    #[error("Manifest error: {0}")]
    Manifest(String),

    /// Wasmtime-specific error.
    #[error("Wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

}

/*impl From<String> for ArcellaWasmtimeError {
    fn from(s: String) -> Self {
        Self::Introspection(s)
    }
}

impl From<&str> for ArcellaWasmtimeError {
    fn from(s: &str) -> Self {
        Self::Introspection(s.into())
    }
}*/
