// arcella/arcella-core/src/config/types.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use indexmap::IndexSet;
use std::collections::HashSet;
use std::path::PathBuf;

use super::ConfigLoadWarning; 

/// Template file suffix
pub const TEMPLATE_TOML_SUFFIX: &str = ".template.toml";

/// The maximum allowed recursion depth when loading configuration files.
///
/// This prevents stack overflow or excessive resource consumption from deeply nested
/// or circular `includes`. The root file is at depth 0, so up to `MAX_CONFIG_DEPTH + 1`
/// files can be loaded in a single inclusion chain.
///
/// Example: with `MAX_CONFIG_DEPTH = 5`, the following is allowed:
/// `root.toml → a.toml → b.toml → c.toml → d.toml → e.toml` (6 files total).
/// Attempting to include a 7th file will trigger a `MaxDepthReached` warning and skip loading.
pub const MAX_CONFIG_DEPTH: usize = 5;

// Immutable parameters — can be freely cloned
#[derive(Debug, Clone)]
pub struct ConfigLoadParams {
    pub prefix: Vec<String>,
    pub config_dir: PathBuf,
}

// Mutable state — passed by &mut reference
pub struct ConfigLoadState {
    /// All configuration files that have been successfully loaded, in order of inclusion.
    pub config_files: IndexSet<PathBuf>,

    /// Tracks files currently in the inclusion stack to detect cyclic includes.
    pub visited_paths: HashSet<PathBuf>,

    /// Non-fatal warnings collected during loading.
    pub warnings: Vec<ConfigLoadWarning>,
}

impl Default for ConfigLoadState {
    fn default() -> Self {
        Self {
            config_files: IndexSet::new(),
            visited_paths: HashSet::new(),
            warnings: Vec::new(),
        }
    }
}
