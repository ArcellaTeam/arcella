// arcella/arcella-types/src/alme/proto/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! ALME (Arcella Local Management Extensions) protocol definitions.
//!
//! This crate defines the shared request/response structures used by both
//! the Arcella daemon (server) and clients (e.g., CLI, GUI, tests).

use serde::{Deserialize, Serialize};

/// A high-level, type-safe ALME command.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "cmd", content = "args")]
pub enum AlmeCommand {
    /// Ping the server: `cmd = "ping"`, args = {}
    Ping,

    /// Tail the log of one or all deployments
    #[serde(rename = "log:tail")]
    LogTail {
        #[serde(default)]
        n: usize,
    },

    /// Get status of one or all deployments
    #[serde(rename = "module:status")]
    Status {
        #[serde(default)]
        deployment_id: Option<String>,
    },

    /// List all deployments
    #[serde(rename = "module:list")]
    ModuleList,

    /// Install a module: `cmd = "module:install"`, args = { "path": "..." }
    #[serde(rename = "module:install")]
    ModuleInstall {
        path: String,
    },

    /// Deploy from file: `cmd = "deploy"`, args = { "file": "..." }
    #[serde(rename = "module:deploy")]
    ModuleDeploy {
        file: String,
    },

    /// Start a deployment by ID
    #[serde(rename = "module:start")]
    ModuleStart {
        deployment_id: String,
    },

    /// Stop a deployment by ID
    #[serde(rename = "module:stop")]
    ModuleStop {
        deployment_id: String,
    },

}

/// An ALME request sent by a client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AlmeRequest {
     #[serde(flatten)]
    pub command: AlmeCommand,
}

/// An ALME response returned by the server.
#[derive(Serialize, Deserialize, Debug)]
pub struct AlmeResponse {
    /// Whether the command succeeded.
    pub success: bool,

    /// Human-readable message (e.g., "pong", "Arcella runtime is active").
    pub message: String,

    /// Optional structured data (e.g., status details, log lines, module list).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl AlmeResponse {
    /// Create a successful response.
    pub fn success(message: &str, data: Option<serde_json::Value>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data,
        }
    }

    /// Create an error response.
    pub fn error(message: &str) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}
