// arcella/arcella-core/src/utils/error.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for `arcella-fs-utils` operations.
pub type ArcellaUtilsResult<T> = std::result::Result<T, ArcellaUtilsError>;

/// Errors that can occur during filesystem and configuration operations.
#[derive(Error, Debug)]
pub enum ArcellaUtilsError {
    /// General-purpose error for unexpected conditions.
    #[error("Internal error: {0}")]
    Internal(String),

    /// I/O error (file not found, permission denied, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// IO error with associated path for better diagnostics
    #[error("I/O error at {path:?}: {source}")]
    IoWithPath {
        source: std::io::Error,
        path: PathBuf,
    },

    /// Invalid argument provided.
    #[error("Invalid argument: {message}")]
    InvalidArgument {
        message: String,
    },

    /// Path not found
    #[error("Path not found: {path:?}")]
    PathNotFound {
        path: PathBuf,
    },

    /// TOML error
    #[error("TOML error: {0}")]
    TOML(String),
}

impl ArcellaUtilsError {
    /// Creates an `IoWithPath` error from a path and an I/O error.
    pub fn io_with_path<E: Into<std::io::Error>>(path: PathBuf, source: E) -> Self {
        Self::IoWithPath {
            source: source.into(),
            path,
        }
    }
}
