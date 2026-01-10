// arcella/arcella-core/src/error/mod.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Centralized error handling for Arcella.
//!
//! Uses `thiserror` to define structured errors and `anyhow` for convenient propagation.
//! All modules should return `Result<T, ArcellaError>` for internal logic,
//! and `anyhow::Result<T>` (aliased as `Result<T>`) for top-level functions like `main`.
//! 

use std::path::PathBuf;
use thiserror::Error;
use tokio::task::JoinError;

use arcella_engine::ArcellaEngineError;
use arcella_types::ArcellaTypeError;
use ministate::MiniStateError;

use crate::{
    utils::ArcellaUtilsError,
};

/// The root error type for all Arcella-specific failures.
#[derive(Error, Debug)]
pub enum ArcellaError {

    // External errors

    /// IO error with associated path for better diagnostics
    #[error("I/O error at {path:?}: {source}")]
    IoWithPath {
        source: std::io::Error,
        path: PathBuf,
    },

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error), 

    /// Tokio task join error.
    #[error("Task join error: {0}")]
    Join(#[from] JoinError),

    /// Tokio lock error.
    #[error("Tokio lock error: {0}")]
    TryLockError(#[from] tokio::sync::TryLockError),

    /// Failed to parse WebAssembly Text Format (`.wat`).
    #[error("WAT parsing error: {0}")]
    Wat(#[from] wat::Error),

    // Internal platform errors

    /// Configuration loading or parsing error.
    #[error("Config error: {0}")]
    ConfigError(String),

    /// Runtime error.
    #[error("Runtime error: {0}")]
    RuntimeError(String),

    /// Invalid argument provided.
    #[error("Invalid argument: {message}")]
    InvalidArgument {
        message: String,
    },
    
    #[error("Arcella Utils error: {0}")]
    UtilsError (#[from] ArcellaUtilsError),  

    /// General-purpose error for unexpected conditions.
    #[error("Internal error: {0}")]
    Internal(String),

    // Specific Arcella errors

    #[error("Module `{0}` is not installed")]
    ModuleNotInstalled(String),

    #[error("Module `{0}` already installed")]
    ModuleAlreadyInstalled(String),

    #[error("Module directory `{0}` already exists on disk")]
    ModuleDirAlreadyExists(String),

    // Inner errors

    #[error("Type error: {0}")]
    TypeError (#[from] ArcellaTypeError),

    #[error("Engine error: {0}")]
    EngineError (#[from] ArcellaEngineError),  

    #[error("MiniState error: {0}")]
    MiniStateError (#[from] MiniStateError), 

}

/// Convenient alias for `Result<T, ArcellaError>`.
///
/// Use this in internal module APIs (e.g., `runtime::install_module`).
pub type ArcellaResult<T> = std::result::Result<T, ArcellaError>;

impl From<std::io::Error> for ArcellaError {
    fn from(e: std::io::Error) -> Self {
        // Fallback: неизвестный путь
        Self::IoWithPath {
            source: e,
            path: PathBuf::from("<unknown>"),
        }
    }
}

// Re-export `anyhow::Result` as `AnyResult` for top-level use (optional but clean)
// Alternatively, you can use `anyhow::Result` directly in `main.rs`
//pub use anyhow::Result as AnyResult;