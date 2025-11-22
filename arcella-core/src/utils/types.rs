// arcella/arcella-core/src/utils/types.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use arcella_types::config::ConfigValues;

/// Maximum allowed recursion depth when traversing nested TOML tables.
///
/// Prevents stack overflow due to deeply nested or maliciously crafted TOML.
/// This limit applies only to table nesting, not array depth or file inclusion depth
/// (which is controlled by `MAX_CONFIG_DEPTH` in `config_loader.rs`).
pub const MAX_TOML_DEPTH: usize = 10;

/// Indicates the outcome of a recursive traversal of a TOML document.
///
/// - `Full`: The entire subtree was processed without hitting depth limits.
/// - `Pruned`: Traversal was stopped early because `MAX_TOML_DEPTH` was exceeded.
///   This is a non-fatal condition; a warning is issued, but loading continues.
#[derive(Debug, Clone, PartialEq)]
pub enum TraversalResult { Full, Pruned }


#[derive(Debug, Clone, PartialEq)]
pub struct TomlFileData {
    pub includes: Vec<String>,
    pub values: ConfigValues,
}
