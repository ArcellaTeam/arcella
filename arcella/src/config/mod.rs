// arcella/arcella/src/config/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Configuration loading, merging, and validation for the Arcella runtime.
//!
//! This module is responsible for:
//! - Locating the base directory and configuration directory.
//! - Ensuring the main configuration file (`arcella.toml`) exists (creating it from a template if needed).
//! - Loading the built-in default configuration and user-provided configuration files recursively.
//! - Merging configuration layers with strict rules for overriding and extension.
//! - Validating the integrity of critical configuration files via modification time (`mtime`) checks.
//!
//! The configuration model enforces a layered approach:
//! 1. **Built-in defaults** — minimal safe defaults shipped with Arcella.
//! 2. **Main config (`arcella.toml`)** — user-editable root configuration.
//! 3. **Included configs** — additional files loaded via `includes` directives.
//!
//! Only keys explicitly marked with `#redef` in a higher-priority layer may be overridden by lower layers.
//! New keys may only be introduced under the `arcella.custom` or `arcella.modules` namespaces.

use futures::future;
use ordered_float::OrderedFloat;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use indexmap::{map::Entry, IndexMap, IndexSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use toml::value::Table;

use arcella_types::{
    config::{
        ConfigValues,
        Value as TomlValue
    }
};
use arcella_fs_utils as fs_utils;

use crate::error::{ArcellaError, Result as ArcellaResult};

/// Standard prefix for all built-in Arcella configuration keys.
pub const ARCELLA_PREFIX: &str = "arcella.";

/// Section name for user-defined custom configuration keys.
///
/// Keys under `arcella.custom.*` may be freely added by users or modules.
const _CUSTOM_SECTION: &str = "custom";

/// Section name for module-specific configuration.
///
/// Keys under `arcella.modules.*` are reserved for dynamic module configuration.
const _MODULES_SECTION: &str = "modules";

/// Full prefix for custom configuration keys.
const CUSTOM_PREFIX_FULL: &str = "arcella.custom";

/// Full prefix for module configuration keys.
const MODULES_PREFIX_FULL: &str = "arcella.modules";

/// Full prefix for log.internal configuration keys.
const LOG_INTERNAL_PREFIX_FULL: &str = "arcella.log.internal";

/// Content of the built-in default configuration (fallback values).
const DEFAULT_CONFIG_CONTENT: &str = include_str!("default_config.toml");

/// Special marker path for the built-in default configuration (not a real file).
///
/// This is **not a real filesystem path**; it is used only as an identifier
/// in the global `config_files` registry.
const DEFAULT_CONFIG_FILENAME: &str = "<builtin:default_config.toml>";

/// Name of the main user-editable configuration file.
const MAIN_CONFIG_FILENAME: &str = "arcella.toml";

/// Suffix used to mark keys that allow redefinition by lower-priority layers.
///
/// Example: `log.level#redef = "debug"` in `arcella.toml` permits  included files
/// to override the `log.level` value.
const REDEF_SUFFIX: &str = "#redef";

/// Content of the template configuration file used for first-time setup.
const TEMPLATE_CONFIG_CONTENT: &str = include_str!("template_config.toml");

/// Configuration integrity metadata parsed from the `[integrity]` section (if present).
#[derive(Deserialize, Default)]
struct IntegrityCheck {
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    dirs: Vec<String>,
}

/// Final resolved Arcella configuration after merging all layers.
#[derive(Debug, Clone)]
pub struct ArcellaConfig {
    /// Flattened key-value configuration map (e.g., `"arcella.log.level"` → `"info"`).
    pub config_values: ConfigValues,

    /// Base directory of the Arcella installation (e.g., parent of `bin/`).
    pub base_dir: PathBuf,

    /// Directory containing `arcella.toml` and included configs.
    pub config_dir: PathBuf,

    /// Integrity checker for critical configuration files.
    pub integrity_checker: IntegrityChecker,
}

impl ArcellaConfig {
    pub fn extract_path_value(&self,  suffix: &str) -> ArcellaResult<PathBuf> {
        extract_path_value(&self.config_values, suffix)
    }
}

/// Tracks file modification times to detect unauthorized changes after startup.
#[derive(Debug, Clone)]
pub struct IntegrityChecker {

    /// List of paths to monitor.
    paths: Vec<PathBuf>,

    /// Initial modification times recorded at startup.
    initial_mtimes: HashMap<PathBuf, SystemTime>,
}

impl IntegrityChecker {
    /// Creates a new integrity checker by recording the current `mtime` of all given paths.
    ///
    /// **Note**: This function is synchronous and blocks the current thread.
    /// For production use, it should be called via `tokio::task::spawn_blocking`.
    pub fn new(paths: Vec<PathBuf>) -> ArcellaResult<Self> {
        // TODO: Keep synchronous for MVP. In the future, consider making it fully asynchronous.
        let mut initial_mtimes = HashMap::new();
        for path in &paths {
            let metadata = std::fs::metadata(path)
                .map_err(|e| ArcellaError::IoWithPath { source: e, path: path.clone() })?;
            let mtime = metadata.modified()
                .map_err(|e| ArcellaError::Internal(format!("Cannot get mtime for {:?}: {}", path, e)))?;
            initial_mtimes.insert(path.clone(), mtime);
        }
        Ok(IntegrityChecker { paths, initial_mtimes })
    }

    /// Checks whether any monitored file has been modified since startup.
    ///
    /// Returns an error if any file's `mtime` differs from the recorded value.
    pub async fn check(&self) -> ArcellaResult<()> {
        let current_mtimes = get_current_mtimes(&self.paths).await?;
        check_mtimes_changed(&self.initial_mtimes, &current_mtimes)
    }
}

/// Adds a warning that a value was ignored because the corresponding key
/// was not marked with `#redef` in the main configuration layer.
///
/// # Arguments
///
/// * `key` – The configuration key that was ignored.
/// * `config_idx` – The index of the config (in the `configs` vector) that lacked the `#redef` permission.
/// * `config_files` – The global set of loaded config files (used to resolve file indices to paths).
/// * `source_file_idx` – The index of the file that attempted to set the value (used for diagnostics).
/// * `warnings` – Mutable vector to which the warning will be appended.
fn add_no_redef_warning(
    key: String,
    config_idx: usize,
    config_files: &IndexSet<PathBuf>,
    source_file_idx: usize,
    warnings: &mut Vec<fs_utils::ConfigLoadWarning>,
) {
    // Resolve the file path that attempted to set the value
    let source_file_path = config_files
        .get_index(source_file_idx)
        .cloned()
        .unwrap_or_else(|| PathBuf::from("unknown"));

    warnings.push(fs_utils::ConfigLoadWarning::ValueError {
        key,
        error: format!(
            "Value from file {:?} ignored due to no #redef flag in layer {}",
            source_file_path, config_idx
        ),
        file: source_file_path,
    });
}

/// Compares initial and current file modification times.
///
/// Returns an error if any file was modified or if a file is missing from the initial list.
fn check_mtimes_changed(
    initial_mtimes: &HashMap<PathBuf, std::time::SystemTime>,
    current_mtimes: &HashMap<PathBuf, std::time::SystemTime>,
) -> ArcellaResult<()> {
    for (path, current_mtime) in current_mtimes {
        if let Some(initial_mtime) = initial_mtimes.get(path) {
            if current_mtime != initial_mtime {
                return Err(ArcellaError::Internal(
                    format!("Config integrity violation: file {:?} was modified after startup", path)
                ));
            }
        } else {
            return Err(ArcellaError::Internal(
                format!("Config integrity violation: file {:?} not found in initial list", path)
            ));
        }
    }
    Ok(())
}

/// Retrieves current modification times for a list of paths concurrently.
async fn get_current_mtimes(paths: &[PathBuf]) -> ArcellaResult<HashMap<PathBuf, std::time::SystemTime>> {
    // Create a vector of futures for each mtime check
    let checks: Vec<_> = paths.iter().map(|path| {
        let path = path.clone();
        async move {
            let metadata = tokio::fs::metadata(&path).await
                .map_err(|e| ArcellaError::IoWithPath { source: e, path: path.clone() })?;
            let mtime = metadata.modified()
                .map_err(|e| ArcellaError::Internal(format!("Cannot get mtime for {:?}: {}", path, e)))?;
            Ok::<(PathBuf, std::time::SystemTime), ArcellaError>((path, mtime))
        }
    }).collect();

    // Run all futures in parallel and wait for completion
    let results = future::join_all(checks).await;

    let mut current_mtimes = HashMap::with_capacity(results.len());
    for result in results {
        // If any check fails, propagate the error
        let (path, mtime) = result?;
        current_mtimes.insert(path, mtime);
    }

    Ok(current_mtimes)
}

/// Ensures that the main configuration file exists, creating it from a template if necessary.
///
/// Also ensures the template file (`arcella.template.toml`) exists.
///
/// **Note**: This function is not atomic and may be subject to race conditions
/// if multiple Arcella instances start simultaneously. For production, consider
/// using `O_CREAT | O_EXCL` semantics via `tokio::fs::OpenOptions::create_new(true)`.
async fn ensure_main_config_exists(config_dir: &Path) -> ArcellaResult<(PathBuf, Vec<fs_utils::ConfigLoadWarning>)> {
    let main_config_path = config_dir.join(MAIN_CONFIG_FILENAME);
    let template_path = config_dir.join("arcella.template.toml");

    let mut warnings: Vec<fs_utils::ConfigLoadWarning> = vec![];

    // Create config_dir if it doesn't exist
    fs::create_dir_all(config_dir)
        .await
        .map_err(|e| ArcellaError::IoWithPath { source: e, path: config_dir.to_path_buf() })?;

    // Create template if missing
    if !template_path.exists() {
        fs::write(&template_path, TEMPLATE_CONFIG_CONTENT)
            .await
            .map_err(|e| ArcellaError::IoWithPath { source: e, path: template_path.clone() })?;
        warnings.push(fs_utils::ConfigLoadWarning::Internal(
            format!("Created default config template at {:?}", template_path)
        ));
    }

    // Create main config from template if missing
    if !main_config_path.exists() {
        // TODO: Potential race condition if another process creates the file between `exists()` and `copy()`.
        fs::copy(&template_path, &main_config_path).await?;
        warnings.push(fs_utils::ConfigLoadWarning::Internal(
            format!("Created default config at {:?}", main_config_path)
        ));
    }

    Ok((main_config_path, warnings))

}

/// Intermediate representation of a resolved configuration value during merging.
struct ResolvedValue {
    value: TomlValue,
    source_config_idx: usize,
    source_file_idx: usize,          // index in `config_files`
    redef_allowed_by: Option<usize>, // file index that granted redefinition permission
}

/// Loads and validates the complete Arcella configuration.
///
/// This is the main entry point for configuration initialization.
/// It performs the following steps:
/// 1. Locates the base directory.
/// 2. Sets up the config directory and ensures `arcella.toml` exists.
/// 3. Loads the built-in default configuration.
/// 4. Recursively loads user configuration and included files.
/// 5. Merges all layers according to Arcella's override rules.
/// 6. Validates required paths and integrity.
///
/// Returns the final `ArcellaConfig` and any non-fatal warnings collected during loading.
pub async fn load() -> ArcellaResult<(ArcellaConfig, Vec<fs_utils::ConfigLoadWarning>)> {
    
    // 1. Find base_dir
    let base_dir = fs_utils::find_base_dir().await?;

    // 2. Set config_dir
    let config_dir = base_dir.join("config");    

    // 3. Ensure config_dir and main config exist
    let (main_config_path, warnings) = ensure_main_config_exists(&config_dir).await?;

    // 4. Prepare paths for integrity checking (currently only main config)
    let integrity_check_paths = vec![main_config_path.clone()];
    let integrity_checker = IntegrityChecker::new(integrity_check_paths)?;
    
    // 5. Initialize loading state
    let mut state  = fs_utils::ConfigLoadState {
        config_files: IndexSet::new(),
        visited_paths: HashSet::new(),
        warnings,
    };

    // 6. Register built-in default config
    let (file_idx, _) = state.config_files.insert_full(
        PathBuf::from(DEFAULT_CONFIG_FILENAME)
    );
    let (default_config, _) = fs_utils::toml::parse_and_collect(
        DEFAULT_CONFIG_CONTENT,
        &vec!["arcella".to_string()],
        file_idx,
    )?;

    // 7. Set up loading parameters
    let params = fs_utils::ConfigLoadParams {
        prefix: vec!["arcella".to_string()],
        config_dir: config_dir.to_path_buf(),
    };

    // 8. Load main config and all included files recursively
    let configs = fs_utils::load_config_recursive_from_file(
        &params,
        &mut state,
        &main_config_path,
    ).await?;

    // 9. Merge configuration layers
    let mut final_values = merge_config(
        &default_config,
        &configs,
        &state.config_files,
        &config_dir,
        &mut state.warnings,
    )?;

    // 10. Sort keys for deterministic output
    final_values.sort_keys();


    integrity_checker.check().await?;

    Ok((
        ArcellaConfig {
            config_values: final_values,
            base_dir,
            config_dir,
            integrity_checker,
        },
        state.warnings,
    ))
}

/// Helper to extract a required string path value from config by suffix.
fn extract_path_value(config: &ConfigValues, suffix: &str) -> ArcellaResult<PathBuf> {
    let full_key = format!("{}{}", ARCELLA_PREFIX, suffix);
    match config.get(&full_key) {
        Some((TomlValue::String(s), _)) => Ok(PathBuf::from(s)),
        _ => Err(ArcellaError::Internal(format!("{} is not set or not a string", full_key))),
    }
}

/// Merges configuration layers according to Arcella's strict override and extension rules.
fn merge_config(
    default_config: &fs_utils::TomlFileData,
    configs: &Vec<fs_utils::TomlFileData>,
    config_files: &IndexSet<PathBuf>,
    config_dir: &Path,
    warnings: &mut Vec<fs_utils::ConfigLoadWarning>
) -> Result< ConfigValues, ArcellaError> {
    
    let mut preliminary_values: IndexMap<String, ResolvedValue> = IndexMap::new();

    // Process from lowest to highest priority (i.e., reverse order of `configs`)
    for config_idx in (0..configs.len()).rev() {
        let config = &configs[config_idx];
        for (key, (value, file_idx)) in &config.values {
            // Check if the key ends with #redef
            let (actual_key, is_redef) = if key.ends_with(REDEF_SUFFIX) {
                // Extract the original key without the #redef suffix
                (key[..key.len() - REDEF_SUFFIX.len()].to_string(), true)
            } else {
                (key.clone(), false)
            };

            match preliminary_values.entry(actual_key.clone()) {
                Entry::Occupied(mut e) => {
                    // Current layer has HIGHER priority (lower idx) than the existing entry
                    if !is_redef { 
                        add_no_redef_warning(
                            actual_key.clone(),
                            config_idx,
                            config_files,
                            e.get().source_file_idx ,
                            warnings,
                        );
                        // Lower-priority layer is ignored; higher-priority layer sets the value.
                        // Overwrite
                        let e = e.get_mut();
                        e.value = value.clone();
                        e.source_config_idx = config_idx;
                        e.source_file_idx = *file_idx;   
                    } else {
                        // The key `actual_key` was already defined in a lower-priority layer
                        e.get_mut().redef_allowed_by = Some(*file_idx);
                    }
                }
                Entry::Vacant(_) => {
                    // No value for this key yet. Store the current value
                    preliminary_values.insert(
                        actual_key, 
                        ResolvedValue {
                            value: value.clone(),
                            source_config_idx: config_idx,
                            source_file_idx: *file_idx,   
                            redef_allowed_by: None,
                        }
                    );
                }
            }

        }
    }

    let main_idx = config_files.get_index_of(&config_dir.join(MAIN_CONFIG_FILENAME)).expect("Main config must be in config_files");
    let default_idx = config_files.get_index_of(
        &PathBuf::from(DEFAULT_CONFIG_FILENAME)
    ).expect("Default config must be in config_files");

    let mut final_values: ConfigValues = IndexMap::new();

    // Initialize final config from default config
    // Perform initial population from the default configuration
    for (key, (value, file_idx)) in &default_config.values {
        final_values.insert(
            key.clone(), 
            (value.clone(), default_idx)
        );  
    }

    // Merge `preliminary_values` with the default config
    // Overriding values from the initial population is allowed only if:
    // - The source is the main config (`source_file == main_idx`), OR
    // - The main config contains this key with the `#redef` suffix (`redef_allowed_by == main_idx`)
    // New keys (not present in the default config) are allowed only under
    // `CUSTOM_PREFIX_FULL`, `MODULES_PREFIX_FULL`  or `LOG_INTERNAL_PREFIX_FULL`
    for (key, preliminary_value) in &preliminary_values {
        // Flag indicating that the configuration section allows
        // adding new keys not present in the default configuration
        let is_newable = key.starts_with(CUSTOM_PREFIX_FULL) 
            || key.starts_with(MODULES_PREFIX_FULL)
            || key.starts_with(LOG_INTERNAL_PREFIX_FULL);
        let new_value = &preliminary_value.value;
        let insert_index = preliminary_value.source_config_idx;

        match final_values.entry(key.clone()) {
            Entry::Occupied(mut entry) => {
                // This key exists in the default configuration
                if preliminary_value.source_file_idx == main_idx {
                    // This value comes from the main config, so it can override the default
                    entry.insert(
                        (new_value.clone(), preliminary_value.source_file_idx)
                    );
                } else if preliminary_value.redef_allowed_by == Some(main_idx) {
                    // Redefinition was allowed by the main config, so override is permitted
                    entry.insert(
                        (new_value.clone(), preliminary_value.source_file_idx)
                    );
                } else {
                    // To override a default value, the main config must mark the key with `#redef`
                    add_no_redef_warning(
                        key.clone(),
                        0,
                        config_files,
                        preliminary_value.source_file_idx,
                        warnings,
                    );
                }
            }
            Entry::Vacant(_) => {
                // This key is absent from the default config, so check
                // that the new key is added under `arcella.custom` or `arcella.modules`
                if is_newable {
                    final_values.insert(
                        key.clone(), 
                        (new_value.clone(), preliminary_value.source_file_idx)
                    );
                } else {
                    // Adding new keys to this section is not allowed
                    warnings.push(fs_utils::ConfigLoadWarning::ValueError {
                        key: key.clone(),
                        error: format!(
                            "Value from layer {} ignored due to missing in default config",
                            insert_index
                        ),
                        file: PathBuf::from(format!("layer_{}.toml", insert_index)),
                    });
                }
            }
        }
    };

    Ok(final_values)

}


/// Extracts a TOML subtree from the flat `ConfigValues` map under a given prefix.
///
/// For example, given:
/// ```toml
/// arcella.log.level = "debug"
/// arcella.log.dir = "/tmp"
/// arcella.server.port = 8080
/// ```
/// Calling `extract_subtree(config, "arcella.log")` returns:
/// ```toml
/// level = "debug"
/// dir = "/tmp"
/// ```
///
/// Nested keys (e.g., `arcella.cache.redis.host`) are converted into nested TOML tables.
pub fn extract_subtree(config: &ConfigValues, prefix: &str) -> Table {
    let mut table = Table::new();
    let prefix_with_dot = format!("{}.", prefix);

    for (key, (value, _)) in config {
        if key.starts_with(&prefix_with_dot) {
            let subkey = &key[prefix_with_dot.len()..];
            if subkey.contains('.') {
                insert_nested(&mut table, subkey, value.clone());
            } else {
                table.insert(subkey.to_string(), toml_value_to_toml(value));
            }
        }
    }

    table
}

/// Recursively inserts a value into a TOML table using a dot-separated path.
///
/// Example: `insert_nested(table, "a.b.c", value)` produces `{ a: { b: { c: value } } }`.
fn insert_nested(table: &mut Table, path: &str, value: TomlValue) {
    let parts: Vec<&str> = path.split('.').collect();
    insert_nested_rec(table, &parts, value);
}

fn insert_nested_rec(table: &mut Table, parts: &[&str], value: TomlValue) {
    if parts.len() == 1 {
        table.insert(parts[0].to_string(), toml_value_to_toml(&value));
    } else {
        let key = parts[0].to_string();
        let entry = table.entry(key).or_insert_with(|| toml::Value::Table(Table::new()));
        if let toml::Value::Table(ref mut subtable) = entry {
            insert_nested_rec(subtable, &parts[1..], value);
        }
        // Ignore type conflicts for MVP (e.g., if key already exists as non-table)
    }
}

/// Converts the internal `TomlValue` enum to `toml::Value`.
fn toml_value_to_toml(value: &TomlValue) -> toml::Value {
    match value {
        TomlValue::String(s) => toml::Value::String(s.clone()),
        TomlValue::Integer(i) => toml::Value::Integer(*i),
        TomlValue::Boolean(b) => toml::Value::Boolean(*b),
        TomlValue::Float(OrderedFloat(f)) => toml::Value::Float(*f),
        // Fallback for unsupported types (arrays, tables) — should not occur in MVP
        _ => toml::Value::String(format!("{:?}", value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_toml_value(s: &str) -> TomlValue {
        TomlValue::String(s.to_string())
    }

    #[test]
    fn test_merge_config_example_from_docs() {
        let config_dir = PathBuf::from("config");
        let mut config_files: IndexSet<PathBuf> = IndexSet::new();

        // Built-in default config (layer 0)
        let (idx, _) = config_files.insert_full(PathBuf::from(DEFAULT_CONFIG_FILENAME));
        let mut default_values: ConfigValues = IndexMap::new();
        default_values.insert("arcella.log.level".to_string(), (make_toml_value("info"), idx));
        default_values.insert("arcella.log.file".to_string(), (make_toml_value("arcella_default.log"), idx));
        default_values.insert("arcella.server.port".to_string(), (make_toml_value("8080"), idx));
        default_values.insert("arcella.server.host".to_string(), (make_toml_value("0.0.0.0"), idx));

        let default_config = fs_utils::TomlFileData {
            includes: vec![],
            values: default_values,
        };

        // arcella.toml (layer 1)
        let (idx, _) = config_files.insert_full(config_dir.join(MAIN_CONFIG_FILENAME));
        let mut main_config_values: ConfigValues = IndexMap::new();
        // level#redef allows redefinition
        main_config_values.insert("arcella.log.level#redef".to_string(), (make_toml_value("warn"), idx));
        main_config_values.insert("arcella.log.file".to_string(), (make_toml_value("arcella_main.log"), idx));
        main_config_values.insert("arcella.server.port".to_string(), (make_toml_value("9000"), idx));

        let main_config = fs_utils::TomlFileData {
            includes: vec![],
            values: main_config_values,
        };

        // level_1.toml (layer 2, assumed to be loaded via includes)
        let (idx, _) = config_files.insert_full(config_dir.join("level_1.toml"));
        let mut level_1_values: ConfigValues = IndexMap::new();
        level_1_values.insert("arcella.log.level".to_string(), (make_toml_value("debug"), idx));
        level_1_values.insert("arcella.server.host".to_string(), (make_toml_value("127.0.0.1"), idx)); // This key is not marked as #redef in arcella.toml -> ignored
        level_1_values.insert("arcella.server.name".to_string(), (make_toml_value("www.server.net"), idx)); // New key in arcella.server -> ignored
        level_1_values.insert("arcella.custom.message".to_string(), (make_toml_value("This is an additional parameter"), idx)); // New key in arcella.custom -> allowed

        let level_1_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_1_values,
        };

        let configs = vec![main_config, level_1_config];

        let mut warnings = vec![];

        let result = merge_config(
            &default_config, 
            &configs, 
            &config_files, 
            &config_dir,
            &mut warnings).expect("merge_config should succeed");

        // Verify final configuration
        assert_eq!(result.get("arcella.log.level"), Some(&(make_toml_value("debug"), 2))); // Overridden from level_1.toml
        assert_eq!(result.get("arcella.log.file"), Some(&(make_toml_value("arcella_main.log"), 1))); // From arcella.toml
        assert_eq!(result.get("arcella.server.port"), Some(&(make_toml_value("9000"), 1))); // From arcella.toml
        assert_eq!(result.get("arcella.server.host"), Some(&(make_toml_value("0.0.0.0"), 0))); // Remains from default_config.toml
        assert_eq!(result.get("arcella.custom.message"), Some(&(make_toml_value("This is an additional parameter"), 2))); // From level_1.toml

        // Verify warnings
        assert_eq!(warnings.len(), 2);

        let warning1 = &warnings[0];
        match warning1 {
            fs_utils::ConfigLoadWarning::ValueError { key, error, .. } => {
                assert_eq!(key, "arcella.server.host");
                assert!(error.contains("ignored due to no #redef flag in layer "));
            }
            _ => panic!("Expected ValueError for arcella.server.host"),
        }

        let warning2 = &warnings[1];
        match warning2 {
            fs_utils::ConfigLoadWarning::ValueError { key, error, .. } => {
                assert_eq!(key, "arcella.server.name");
                assert!(error.contains("ignored due to missing in default config"));
            }
            _ => panic!("Expected ValueError for arcella.server.name"),
        }
    }

    #[test]
    fn test_merge_config_no_redef_prevents_override() {
        let config_dir = PathBuf::from("config");
        let mut config_files: IndexSet<PathBuf> = IndexSet::new();

        // default_config (layer 0)
        let (idx, _) = config_files.insert_full(PathBuf::from(DEFAULT_CONFIG_FILENAME));
        let mut default_values: ConfigValues = IndexMap::new();
        default_values.insert("arcella.server.host".to_string(), (make_toml_value("0.0.0.0"), idx));
        default_values.insert("arcella.server.port".to_string(), (make_toml_value("8090"), idx));
        let default_config = fs_utils::TomlFileData {
            includes: vec![],
            values: default_values,
        };

        // arcella.toml (layer 1) - does not mark host as #redef
        let (idx, _) = config_files.insert_full(config_dir.join(MAIN_CONFIG_FILENAME));
        let mut main_config_values: ConfigValues = IndexMap::new();
        main_config_values.insert("arcella.server.host".to_string(), (make_toml_value("192.168.1.1"), idx));
        let main_config = fs_utils::TomlFileData {
            includes: vec![],
            values: main_config_values,
        };

        // level_1.toml (layer 2)
        let (idx, _) = config_files.insert_full(config_dir.join("level_1.toml"));
        let mut level_1_values: ConfigValues = IndexMap::new();
        level_1_values.insert("arcella.server.port".to_string(), (make_toml_value("9000"), idx));
        let level_1_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_1_values,
        };

        // level_2.toml (layer 3) - attempts to change host
        let (idx, _) = config_files.insert_full(config_dir.join("level_2.toml"));
        let mut level_2_values: ConfigValues = IndexMap::new();
        level_2_values.insert("arcella.server.host".to_string(), (make_toml_value("127.0.0.1"), idx));
        let level_2_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_2_values,
        };

        let configs = vec![main_config, level_1_config, level_2_config];

        let mut warnings = vec![];

        let result = merge_config(
            &default_config, 
            &configs, 
            &config_files, 
            &config_dir,
            &mut warnings).expect("merge_config should succeed");

        assert_eq!(result.get("arcella.server.host"), Some(&(make_toml_value("192.168.1.1"), 1))); // Remains value from arcella.toml

        assert_eq!(warnings.len(), 2);
        let warning_1 = &warnings[0];
        match warning_1 {
            fs_utils::ConfigLoadWarning::ValueError { key, error, .. } => {
                assert_eq!(key, "arcella.server.host");
                assert!(error.contains("Value from file \"config/level_2.toml\" ignored due to no #redef flag in layer 0"));
            }
            _ => panic!("Expected ValueError for arcella.server.host due to missing #redef in arcella.toml when layer 2 tried to set it"),
        }
        let warning_2 = &warnings[1];
        match warning_2 {
            fs_utils::ConfigLoadWarning::ValueError { key, error, .. } => {
                assert_eq!(key, "arcella.server.port");
                assert!(error.contains("Value from file \"config/level_1.toml\" ignored due to no #redef flag in layer 0"));
            }
            _ => panic!("Expected ValueError for arcella.server.port due to missing #redef in arcella.toml when layer 1 tried to set it"),
        }
    }

    #[test]
    fn test_merge_config_redef_allows_override() {
        let config_dir = PathBuf::from("config");
        let mut config_files: IndexSet<PathBuf> = IndexSet::new();

        // default_config (layer 0)
        let (idx, _) = config_files.insert_full(PathBuf::from(DEFAULT_CONFIG_FILENAME));
        let mut default_values: ConfigValues = IndexMap::new();
        default_values.insert("arcella.log.level".to_string(), (make_toml_value("info"), idx));
        let default_config = fs_utils::TomlFileData {
            includes: vec![],
            values: default_values,
        };

        // arcella.toml (layer 1) - marks level as #redef
        let (idx, _) = config_files.insert_full(config_dir.join(MAIN_CONFIG_FILENAME));
        let mut main_config_values: ConfigValues = IndexMap::new();
        main_config_values.insert("arcella.log.level#redef".to_string(), (make_toml_value("warn"), idx));
        let main_config = fs_utils::TomlFileData {
            includes: vec![],
            values: main_config_values,
        };

        // level_1.toml (layer 2) - can change level because arcella.toml marked it as #redef
        let (idx, _) = config_files.insert_full(config_dir.join("level_1.toml"));
        let mut level_1_values: ConfigValues = IndexMap::new();
        level_1_values.insert("arcella.log.level#redef".to_string(), (make_toml_value("debug"), idx));
        let level_1_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_1_values,
        };

        // level_2.toml (layer 3) - can change level because level_1.toml marked it as #redef
        let (idx, _) = config_files.insert_full(config_dir.join("level_2.toml"));
        let mut level_2_values: ConfigValues = IndexMap::new();
        level_2_values.insert("arcella.log.level".to_string(), (make_toml_value("trace"), idx));
        let level_2_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_2_values,
        };

        let configs = vec![main_config, level_1_config, level_2_config];

        let mut warnings = vec![];

        let result = merge_config(
            &default_config, 
            &configs, 
            &config_files, 
            &config_dir,
            &mut warnings).expect("merge_config should succeed");

        // The level value should be overridden from level_1.toml because #redef allowed it in arcella.toml
        assert_eq!(result.get("arcella.log.level"), Some(&(make_toml_value("trace"), 3)));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_merge_config_new_key_in_custom_allowed() {
        let config_dir = PathBuf::from("config");
        let mut config_files: IndexSet<PathBuf> = IndexSet::new();

        // default_config (layer 0)
        let (idx, _) = config_files.insert_full(PathBuf::from(DEFAULT_CONFIG_FILENAME));
        let mut default_values: ConfigValues = IndexMap::new();
        default_values.insert("arcella.log.level".to_string(), (make_toml_value("info"), idx));
        let default_config = fs_utils::TomlFileData {
            includes: vec![],
            values: default_values,
        };

        // arcella.toml (layer 1)
        let (idx, _) = config_files.insert_full(config_dir.join(MAIN_CONFIG_FILENAME));
        let main_config_values: ConfigValues = IndexMap::new(); // Empty
        let main_config = fs_utils::TomlFileData {
            includes: vec![],
            values: main_config_values,
        };

        // level_1.toml (layer 2) - adds a new key under arcella.custom
        let (idx, _) = config_files.insert_full(config_dir.join("level_1.toml"));
        let mut level_1_values: ConfigValues = IndexMap::new();
        level_1_values.insert("arcella.custom.new_key".to_string(), (make_toml_value("new_value"), idx));
        let level_1_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_1_values,
        };

        let configs = vec![main_config, level_1_config];

        let mut warnings = vec![];

        let result = merge_config(
            &default_config, 
            &configs, 
            &config_files, 
            &config_dir,
            &mut warnings).expect("merge_config should succeed");

        assert_eq!(result.get("arcella.custom.new_key"), Some(&(make_toml_value("new_value"), 2)));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_merge_config_new_key_in_server_ignored() {
        // default_config (layer 0)
        let mut default_values: ConfigValues = IndexMap::new();
        default_values.insert("arcella.log.level".to_string(), (make_toml_value("info"), 0));
        let default_config = fs_utils::TomlFileData {
            includes: vec![],
            values: default_values,
        };

        // arcella.toml (layer 1)
        let main_config_values: ConfigValues = IndexMap::new(); // Empty
        let main_config = fs_utils::TomlFileData {
            includes: vec![],
            values: main_config_values,
        };

        // level_1.toml (layer 2) - attempts to add a new key under arcella.server
        let mut level_1_values: ConfigValues = IndexMap::new();
        level_1_values.insert("arcella.server.new_option".to_string(), (make_toml_value("some_value"), 2));
        let level_1_config = fs_utils::TomlFileData {
            includes: vec![],
            values: level_1_values,
        };

        let configs = vec![main_config, level_1_config];

        let mut warnings = vec![];

        let config_dir = PathBuf::from("config");
        let mut config_files: IndexSet<PathBuf> = IndexSet::new();
        config_files.insert(PathBuf::from(DEFAULT_CONFIG_FILENAME));
        config_files.insert(config_dir.join(MAIN_CONFIG_FILENAME));
        config_files.insert(config_dir.join("level_1.toml"));

        let result = merge_config(
            &default_config, 
            &configs, 
            &config_files, 
            &config_dir,
            &mut warnings).expect("merge_config should succeed");

        // The new key should not appear
        assert!(!result.contains_key("arcella.server.new_option"));
        // A warning should be issued
        assert_eq!(warnings.len(), 1);
        let warning = &warnings[0];
        match warning {
            fs_utils::ConfigLoadWarning::ValueError { key, error, .. } => {
                assert_eq!(key, "arcella.server.new_option");
                assert!(error.contains("ignored due to missing in default config"));
            }
            _ => panic!("Expected ValueError for new key in arcella.server"),
        }
    }    

    #[test]
    fn test_extract_subtree() {
        let mut config = ConfigValues::new();
        config.insert("arcella.log.level".to_string(), (TomlValue::String("debug".into()), 0));
        config.insert("arcella.log.dir".to_string(), (TomlValue::String("/tmp".into()), 0));
        config.insert("arcella.server.port".to_string(), (TomlValue::Integer(8080), 0));

        let subtree = extract_subtree(&config, "arcella.log");
        assert_eq!(subtree["level"].as_str(), Some("debug"));
        assert_eq!(subtree["dir"].as_str(), Some("/tmp"));
        assert!(!subtree.contains_key("port"));
    }

    #[test]
    fn test_extract_nested_subtree() {
        let mut config = ConfigValues::new();
        config.insert("arcella.cache.redis.host".to_string(), (TomlValue::String("localhost".into()), 0));
        config.insert("arcella.cache.redis.port".to_string(), (TomlValue::Integer(6379), 0));

        let subtree = extract_subtree(&config, "arcella.cache");
        assert_eq!(subtree["redis"]["host"].as_str(), Some("localhost"));
        assert_eq!(subtree["redis"]["port"].as_integer(), Some(6379));
    }    

}
