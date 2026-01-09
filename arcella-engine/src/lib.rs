// arcella/arcella-engine/src/lib.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! # WebAssembly Engine-Agnostic Interface
//!
//! This module defines **universal abstractions** over WebAssembly engines,
//! allowing Arcella to work with different implementations (Wasmtime, Wazero, Wasmer, etc.),
//! while maintaining **compatibility and adaptability**.
//!
//! ## Core Objectives
//!
//! - **Graceful Degradation**: If a requested feature (e.g., Component Model)
//!   is not supported by the engine, Arcella does not crash but operates in the most capable mode available.
//! - **Configuration Profiles**: Predefined feature sets (e.g., `WasiOnly`, `HighPerformance`)
//!   simplify configuration for various usage scenarios.
//! - **Diagnostics**: Instead of a hard failure, the engine returns a **compatibility report**
//!   detailing which features are supported, requested, and which trigger warnings.
//!
//! ## Architecture
//!
//! 1. **`WasmFeature`** — an enumeration of all known WebAssembly features.
//! 2. **`WasmFeatureGroup`** — predefined usage profiles.
//! 3. **`WasmEngineConfig`** — engine configuration, including individual features and a profile.
//! 4. **`WasmEngineCapabilities`** — a trait describing what the engine *can do*.
//! 5. **`WasmEngine`** — the main trait for integrating an engine into Arcella.
//! 6. **`WasmEngineCompatibilityReport`** — a report on the compatibility between configuration and engine capabilities.
//!
//! ## Usage Example
//!
//! ```rust, ignore
//! use arcella_core::engine::{WasmEngineConfig, WasmFeatureGroup};
//!
//! // Standard profile for Component Model
//! let config = WasmEngineConfig::default()
//!     .with_profile(WasmFeatureGroup::ComponentModel)
//!     .enable_threads(false);
//!
//! // Validation (optional)
//! config.validate().expect("valid config");
//!
//! // ... pass the config to an engine adapter (e.g., WasmtimeEngine)
//! ```
//!
//! See also: [`arcella_types::manifest::ComponentManifest`], [`wasmtime::Engine`]

use derive_builder::Builder;
use std::path::Path;

use arcella_types::manifest::ComponentManifest;

mod error;
pub use error::{ArcellaEngineError, ArcellaEngineResult};

// =============================================================================
// 1. Typed feature list
// =============================================================================

/// An enumeration of all WebAssembly features supported by Arcella.
///
/// Each feature has:
/// - a unique Arcella-specific name (`arcella_name()`),
/// - a human-readable description (`description()`),
/// - a method to extract its value from a configuration (`get_config_value`).
///
/// This is the central registry for features— all engines and profiles must use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmFeature {
    /// WebAssembly Component Model (WIT interfaces, strong typing).
    ComponentModel,
    /// Reference types (`externref`, `funcref`).
    ReferenceTypes,
    /// Bulk memory operations (`memory.copy`, `memory.fill`, `memory.init`).
    BulkMemory,
    /// 128-bit SIMD instructions.
    Simd,
    /// WebAssembly Garbage Collection (GC).
    Gc,
    /// Functions returning multiple values.
    MultiValue,
    /// Threading and shared memory support.
    Threads,
    /// Tail calls.
    TailCall,
    /// Typed function references.
    FunctionReferences,
}

impl WasmFeature {
    /// Returns the canonical Arcella-specific name of the feature.
    ///
    /// Used in configuration, logs, and diagnostics.
    pub const fn arcella_name(&self) -> &'static str {
        match self {
            Self::ComponentModel => "component_model",
            Self::ReferenceTypes => "reference_types",
            Self::BulkMemory => "bulk_memory",
            Self::Simd => "simd",
            Self::Gc => "gc",
            Self::MultiValue => "multi_value",
            Self::Threads => "threads",
            Self::TailCall => "tail_call",
            Self::FunctionReferences => "function_references",
        }
    }

    /// Returns a human-readable description of the feature.
    pub const fn description(&self) -> &'static str {
        match self {
            Self::ComponentModel => "WebAssembly Component Model",
            Self::ReferenceTypes => "Reference types (externref, funcref)",
            Self::BulkMemory => "Bulk memory operations (memory.copy, memory.fill)",
            Self::Simd => "128-bit SIMD instructions",
            Self::Gc => "WebAssembly Garbage Collection",
            Self::MultiValue => "Functions returning multiple values",
            Self::Threads => "Threading and shared memory",
            Self::TailCall => "Tail call optimization",
            Self::FunctionReferences => "Typed function references",
        }
    }

    /// Extracts the feature's value from the given configuration.
    ///
    /// Returns `None` if the value is not set (the engine may decide whether to enable the feature).
    pub fn get_config_value(&self, config: &WasmEngineConfig) -> Option<bool> {
        match self {
            Self::ComponentModel => config.enable_component_model,
            Self::ReferenceTypes => config.enable_reference_types,
            Self::BulkMemory => config.enable_bulk_memory,
            Self::Simd => config.enable_simd,
            Self::Gc => config.enable_gc,
            Self::MultiValue => config.enable_multi_value,
            Self::Threads => config.enable_threads,
            Self::TailCall => config.enable_tail_call,
            Self::FunctionReferences => config.enable_function_references,
        }
    }

    /// Returns a complete list of all features.
    pub const fn all() -> &'static [Self] {
        &[
            Self::ComponentModel,
            Self::ReferenceTypes,
            Self::BulkMemory,
            Self::Simd,
            Self::Gc,
            Self::MultiValue,
            Self::Threads,
            Self::TailCall,
            Self::FunctionReferences,
        ]
    }
}

// =============================================================================
// 2. Feature groups (profiles)
// =============================================================================

/// Predefined profiles (feature sets) for typical usage scenarios.
///
/// A profile is a convenient way to specify a recommended set of features without manual configuration of each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmFeatureGroup {
    /// WASI only (core module without Component Model).
    /// Suitable for legacy modules and embedded devices.
    WasiOnly,

    /// Standard Arcella profile: Component Model + basic features.
    /// Used by default for most components.
    ComponentModel,

    /// Full Wasm GC support (experimental).
    /// Required for components written in GC languages (e.g., Koto, experimental Rust).
    GcFull,

    /// Minimal feature set for resource-constrained environments.
    /// Only `multi_value` (often required even in simple modules).
    EmbeddedMinimal,

    /// High-performance profile:
    /// SIMD, threads, bulk memory.
    HighPerformance,

    /// Hardened security profile:
    /// no shared memory, threads, or JIT.
    /// Suitable for multi-tenant environments.
    Secure,

    /// An experimental feature set (empty by default).
    /// Can be used for testing new capabilities.
    Experimental,
}

impl WasmFeatureGroup {
    /// Returns the list of features included in the profile.
    pub const fn features(&self) -> &'static [WasmFeature] {
        use WasmFeature::*;
        match self {
            Self::WasiOnly => &[MultiValue, BulkMemory, ReferenceTypes],
            Self::ComponentModel => &[ComponentModel, ReferenceTypes, MultiValue, BulkMemory],
            Self::GcFull => &[ComponentModel, ReferenceTypes, Gc, FunctionReferences, MultiValue],
            Self::EmbeddedMinimal => &[MultiValue],
            Self::HighPerformance => &[ComponentModel, Simd, MultiValue, BulkMemory, Threads],
            Self::Secure => &[ComponentModel, ReferenceTypes, MultiValue],
            Self::Experimental => &[],
        }
    }

    /// Returns a description of the profile.
    pub const fn description(&self) -> &'static str {
        match self {
            Self::WasiOnly => "WASI core modules only (no Component Model)",
            Self::ComponentModel => "Standard Component Model with WIT interfaces",
            Self::GcFull => "Full Wasm GC support (experimental)",
            Self::EmbeddedMinimal => "Minimal feature set for resource-constrained environments",
            Self::HighPerformance => "High-performance profile (SIMD, threads, bulk memory)",
            Self::Secure => "Security-hardened profile (no shared memory or JIT)",
            Self::Experimental => "Experimental feature set",
        }
    }
}

// =============================================================================
// 3. Engine configuration
// =============================================================================

/// Universal configuration for a WebAssembly engine.
///
/// Fields are `Option<bool>` to distinguish three states:
/// - `Some(true)` — the feature **must** be enabled,
/// - `Some(false)` — the feature **must** be disabled,
/// - `None` — the engine may decide whether to enable or disable the feature.
///
/// It also supports setting a `profile`, which automatically sets feature values.
#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned", setter(into))]
pub struct WasmEngineConfig {
    /// Maximum number of 64KiB memory pages (usually 1 page = 64 KiB).
    pub max_memory_pages: Option<u32>,

    /// Bulk memory operations.
    pub enable_bulk_memory: Option<bool>,

    /// Reference types.
    pub enable_reference_types: Option<bool>,

    /// SIMD.
    pub enable_simd: Option<bool>,

    /// Multi-value.
    pub enable_multi_value: Option<bool>,

    /// Component Model.
    pub enable_component_model: Option<bool>,

    /// Threading.
    #[builder(default)]
    pub enable_threads: Option<bool>,

    /// Tail call.
    pub enable_tail_call: Option<bool>,

    /// Function references.
    pub enable_function_references: Option<bool>,

    /// Garbage Collection.
    pub enable_gc: Option<bool>,

    /// The profile from which feature values will be inherited.
    pub profile: Option<WasmFeatureGroup>,
}

impl Default for WasmEngineConfig {
    /// Returns the default configuration:
    /// - 1 GiB of memory (16,384 pages),
    /// - all features are `None` (the engine decides).
    fn default() -> Self {
        Self {
            max_memory_pages: Some(16_384),
            enable_bulk_memory: None,
            enable_reference_types: None,
            enable_simd: None,
            enable_multi_value: None,
            enable_component_model: None,
            enable_threads: None,
            enable_tail_call: None,
            enable_function_references: None,
            enable_gc: None,
            profile: None,
        }
    }
}

impl WasmEngineConfig {
    /// A reasonable maximum memory limit: 4 GiB.
    pub const MAX_REASONABLE_PAGES: u32 = 65536;

    /// A reasonable minimum memory limit: 64 KiB.
    pub const MIN_REASONABLE_PAGES: u32 = 1;

    /// Explicitly enables or disables threading support.
    pub fn enable_threads(mut self, value: bool) -> Self {
        self.enable_threads = Some(value);
        self
    }    

    /// Applies a configuration profile, setting feature values
    /// **only if they have not been explicitly set already**.
    ///
    /// This allows you to set a profile first and then override
    /// specific features using the builder chain.
    pub fn with_profile(mut self, profile: WasmFeatureGroup) -> Self {
        self.profile = Some(profile);

        for &feature in profile.features() {
            match feature {
                WasmFeature::ComponentModel if self.enable_component_model.is_none() => {
                    self.enable_component_model = Some(true);
                }
                WasmFeature::ReferenceTypes if self.enable_reference_types.is_none() => {
                    self.enable_reference_types = Some(true);
                }
                WasmFeature::Simd if self.enable_simd.is_none() => {
                    self.enable_simd = Some(true);
                }
                WasmFeature::Gc if self.enable_gc.is_none() => {
                    self.enable_gc = Some(true);
                }
                WasmFeature::Threads if self.enable_threads.is_none() => {
                    self.enable_threads = Some(true);
                }
                WasmFeature::BulkMemory if self.enable_bulk_memory.is_none() => {
                    self.enable_bulk_memory = Some(true);
                }
                WasmFeature::MultiValue if self.enable_multi_value.is_none() => {
                    self.enable_multi_value = Some(true);
                }
                WasmFeature::TailCall if self.enable_tail_call.is_none() => {
                    self.enable_tail_call = Some(true);
                }
                WasmFeature::FunctionReferences if self.enable_function_references.is_none() => {
                    self.enable_function_references = Some(true);
                }
                _ => {}
            }
        }

        self
    }

    /// Validates the logical correctness of the configuration.
    ///
    /// Currently, only memory size limits are checked.
    /// Compatibility with a specific engine is checked separately via `compatibility_report`.
    pub fn validate(&self) -> Result<(), ArcellaEngineError> {
        if let Some(pages) = self.max_memory_pages {
            if pages < Self::MIN_REASONABLE_PAGES {
                return Err(ArcellaEngineError::MemoryTooSmall(pages));
            }
            if pages > Self::MAX_REASONABLE_PAGES {
                return Err(ArcellaEngineError::MemoryTooLarge(pages));
            }
        }
        Ok(())
    }
}

// =============================================================================
// 4. Compatibility report
// =============================================================================

/// Information about a specific feature's support by the engine.
#[derive(Debug, Clone)]
pub struct SupportedFeature {
    /// The feature's name in Arcella terms (e.g., `"component_model"`).
    pub arcella_name: &'static str,

    /// Whether the feature was requested in the configuration.
    pub requested: bool,

    /// Whether the feature is supported by the engine.
    pub supported: bool,

    /// The feature's name in the specific engine's terms (e.g., `"wasm_component_model"` for Wasmtime).
    pub engine_specific_name: String,

    /// Additional information (e.g., "only on x86_64").
    pub notes: Option<String>,
}

/// A compatibility report between the configuration and the engine's capabilities.
///
/// Used for:
/// - informative logging,
/// - making decisions about execution (e.g., can WIT interfaces be used?),
/// - deployment diagnostics.
#[derive(Debug, Clone, Default)]
pub struct WasmEngineCompatibilityReport {
    /// The profile used in the configuration (if set).
    pub profile: Option<WasmFeatureGroup>,

    /// A list of all features with detailed support information.
    pub features: Vec<SupportedFeature>,

    /// A list of non-critical incompatibilities (warnings).
    pub warnings: Vec<String>,
}

impl WasmEngineCompatibilityReport {
    /// Checks for a **critical incompatibility**:
    /// the Component Model feature was requested, but the engine does not support it.
    ///
    /// This is important because without Component Model, working with WIT interfaces is impossible,
    /// and installing a component that depends on them would fail.
    pub fn has_critical_gaps(&self) -> bool {
        self.features.iter().any(|f| {
            f.arcella_name == "component_model" && f.requested && !f.supported
        })
    }

    /// Indicates whether the engine can be used, even if not all features are available.
    ///
    /// Arcella always strives to operate in the most capable mode possible,
    /// so this method always returns `true`.
    pub fn is_runnable(&self) -> bool {
        true
    }
}

// =============================================================================
// 5. Engine capabilities trait
// =============================================================================

/// A trait describing the capabilities of a specific WebAssembly engine.
///
/// An implementation must provide information about which features the engine supports,
/// including their names and any specifics of their support.
pub trait WasmEngineCapabilities {
    /// Returns a list of all features with detailed support information.
    fn supported_features(&self) -> Vec<SupportedFeature>;

    /// Generates a compatibility report between the given configuration and
    /// the engine's capabilities.
    ///
    /// This method **never panics**: even if unsupported features are requested,
    /// it returns a report with warnings.
    fn compatibility_report(&self, config: &WasmEngineConfig) -> WasmEngineCompatibilityReport {
        let mut report = WasmEngineCompatibilityReport::default();
        report.profile = config.profile;

        let known_features = self.supported_features();

        for &feature in WasmFeature::all() {
            let requested = feature.get_config_value(config).unwrap_or(false);
            let arcella_name = feature.arcella_name();

            if let Some(feat) = known_features.iter().find(|f| f.arcella_name == arcella_name) {
                if requested && !feat.supported {
                    report.warnings.push(format!(
                        "requested feature '{}' ({}) is not supported{}",
                        arcella_name,
                        feat.engine_specific_name,
                        feat.notes.as_ref().map_or(String::new(), |n| format!(": {}", n))
                    ));
                }
                report.features.push(SupportedFeature {
                    arcella_name,
                    requested,
                    supported: feat.supported,
                    engine_specific_name: feat.engine_specific_name.clone(),
                    notes: feat.notes.clone(),
                });
            } else {
                // The feature is not supported by the engine at all
                report.warnings.push(format!(
                    "requested feature '{}' is not recognized by engine",
                    arcella_name
                ));
                report.features.push(SupportedFeature {
                    arcella_name,
                    requested,
                    supported: false,
                    engine_specific_name: "unknown".to_string(),
                    notes: Some("not implemented in this engine".to_string()),
                });
            }
        }

        report
    }
}

// =============================================================================
// 6. Main engine trait
// =============================================================================

/// The main trait for integrating a WebAssembly engine into Arcella.
///
/// Any engine that needs to work with Arcella must implement this trait.
/// The implementation must be **Send + Sync** since the engine is used in a multi-threaded environment (tokio).
#[allow(async_fn_in_trait)]
pub trait WasmEngine: WasmEngineCapabilities + Send + Sync {
    /// Performs introspection on a WASM file and returns its manifest.
    ///
    /// The method must correctly handle both core modules (WASI) and Component Model.
    /// If the engine does not support Component Model, it **must** return a manifest of type `CoreWasi`.
    async fn inspect_component(&self, wasm_path: &Path) -> ArcellaEngineResult<ComponentManifest>;

    /// Returns a short engine name (e.g., `"wasmtime"`).
    fn name(&self) -> &'static str;

    /// Returns the engine version (e.g., `"12.0.1"`).
    fn version(&self) -> &'static str;

    /// Returns the version of the Arcella Engine API implemented by the engine.
    ///
    /// Used for compatibility checks when dynamically loading engines.
    fn api_version(&self) -> u32;
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // ... (the original test code remains unchanged)
    use super::*;
    use std::collections::HashMap;

    struct MockWasmEngine {
        supported_features_map: HashMap<&'static str, (bool, String, Option<String>)>,
    }

    impl MockWasmEngine {
        fn new() -> Self {
            let mut map = HashMap::new();
            map.insert(
                "component_model",
                (true, "wasm_component_model".to_string(), Some("Wasmtime >=12".to_string())),
            );
            map.insert(
                "reference_types",
                (true, "wasm_reference_types".to_string(), None),
            );
            map.insert(
                "simd",
                (false, "wasm_simd".to_string(), Some("only on x86_64".to_string())),
            );
            map.insert(
                "gc",
                (false, "wasm_gc".to_string(), Some("not implemented".to_string())),
            );
            map.insert(
                "bulk_memory",
                (true, "wasm_bulk_memory".to_string(), None),
            );
            map.insert(
                "multi_value",
                (true, "wasm_multi_value".to_string(), None),
            );
            map.insert(
                "threads",
                (false, "wasm_threads".to_string(), Some("disabled by default".to_string())),
            );
            map.insert(
                "tail_call",
                (true, "wasm_tail_call".to_string(), None),
            );
            map.insert(
                "function_references",
                (false, "wasm_function_references".to_string(), None),
            );
            Self { supported_features_map: map }
        }
    }

    impl WasmEngineCapabilities for MockWasmEngine {
        fn supported_features(&self) -> Vec<SupportedFeature> {
            WasmFeature::all()
                .iter()
                .map(|&feature| {
                    let arcella_name = feature.arcella_name();
                    if let Some((supported, engine_name, notes)) = self.supported_features_map.get(arcella_name) {
                        SupportedFeature {
                            arcella_name,
                            requested: false,
                            supported: *supported,
                            engine_specific_name: engine_name.clone(),
                            notes: notes.clone(),
                        }
                    } else {
                        SupportedFeature {
                            arcella_name,
                            requested: false,
                            supported: false,
                            engine_specific_name: "unknown".to_string(),
                            notes: Some("not configured in mock".to_string()),
                        }
                    }
                })
                .collect()
        }
    }

    impl WasmEngine for MockWasmEngine {
        async fn inspect_component(&self, _wasm_path: &Path) -> ArcellaEngineResult<ComponentManifest> {
            unimplemented!("not used in these tests")
        }

        fn name(&self) -> &'static str {
            "mock_engine"
        }

        fn version(&self) -> &'static str {
            "0.1.0"
        }

        fn api_version(&self) -> u32 {
            1
        }
    }


    #[test]
    fn test_default_config() {
        let config = WasmEngineConfig::default();
        assert_eq!(config.enable_component_model, None);
        assert_eq!(config.enable_simd, None);
        assert_eq!(config.profile, None);
        assert_eq!(config.max_memory_pages, Some(16_384));
    }

    #[test]
    fn test_config_with_profile_wasi_only() {
        let config = WasmEngineConfig::default()
        .with_profile(WasmFeatureGroup::WasiOnly);

        assert_eq!(config.enable_component_model, None); // not part of WasiOnly
        assert_eq!(config.enable_bulk_memory, Some(true));
        assert_eq!(config.enable_reference_types, Some(true));
        assert_eq!(config.enable_multi_value, Some(true));
        assert_eq!(config.profile, Some(WasmFeatureGroup::WasiOnly));
    }

    #[test]
    fn test_config_with_profile_component_model() {
        let config = WasmEngineConfig::default()
        .with_profile(WasmFeatureGroup::ComponentModel);

        assert_eq!(config.enable_component_model, Some(true));
        assert_eq!(config.enable_bulk_memory, Some(true));
        assert_eq!(config.enable_reference_types, Some(true));
        assert_eq!(config.enable_multi_value, Some(true));
        assert_eq!(config.profile, Some(WasmFeatureGroup::ComponentModel));
    }

    #[test]
    fn test_config_profile_overrides_none_only() {
        let config = WasmEngineConfig {
            enable_simd: None,
            enable_multi_value: None,
            enable_gc: Some(false),
            ..Default::default()
        }
        .with_profile(WasmFeatureGroup::GcFull);

        assert_eq!(config.enable_simd, None);
        assert_eq!(config.enable_multi_value, Some(true));
        assert_eq!(config.enable_gc, Some(false));
    }

    #[test]
    fn test_memory_validation() {
        let mut config = WasmEngineConfig::default();
        config.max_memory_pages = Some(0);
        assert!(config.validate().is_err());

        config.max_memory_pages = Some(WasmEngineConfig::MAX_REASONABLE_PAGES + 1);
        assert!(config.validate().is_err());

        config.max_memory_pages = Some(1024);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_compatibility_report_no_gaps() {
        let engine = MockWasmEngine::new();
        let config = WasmEngineConfig {
            enable_component_model: Some(true),
            enable_reference_types: Some(true),
            enable_bulk_memory: Some(true),
            enable_multi_value: Some(true),
            enable_simd: Some(false), // explicitly disabled
            enable_gc: Some(false),
            ..Default::default()
        };

        let report = engine.compatibility_report(&config);

        assert!(!report.has_critical_gaps());
        assert!(report.is_runnable());

        // Verify that SIMD and GC did not cause warnings (they are disabled)
        assert!(!report.warnings.iter().any(|w| w.contains("simd") || w.contains("gc")));
    }

    #[test]
    fn test_compatibility_report_with_warnings() {
        let engine = MockWasmEngine::new();
        let config = WasmEngineConfig {
            enable_simd: Some(true),
            enable_gc: Some(true),
            enable_threads: Some(true),
            ..Default::default()
        };

        let report = engine.compatibility_report(&config);

        assert_eq!(report.warnings.len(), 3);
        assert!(report.warnings.iter().any(|w| w.contains("simd")));
        assert!(report.warnings.iter().any(|w| w.contains("gc")));
        assert!(report.warnings.iter().any(|w| w.contains("threads")));
    }

    #[test]
    fn test_compatibility_report_critical_gap() {
        let engine = MockWasmEngine::new();
        let config = WasmEngineConfig {
            enable_component_model: Some(true),
            ..Default::default()
        };

        let report = engine.compatibility_report(&config);

        // In the mock, component_model = true → no critical gap
        assert!(!report.has_critical_gaps());

        // Now create an engine that does NOT support Component Model
        let mut broken_engine = MockWasmEngine::new();
        broken_engine.supported_features_map.insert(
            "component_model",
            (false, "wasm_component_model".to_string(), Some("disabled".to_string())),
        );

        let report2 = broken_engine.compatibility_report(&config);
        assert!(report2.has_critical_gaps());
    }

    #[test]
    fn test_wasm_feature_accessors() {
        let config = WasmEngineConfig {
            enable_component_model: Some(true),
            enable_simd: None,
            enable_gc: Some(false),
            ..Default::default()
        };

        assert_eq!(WasmFeature::ComponentModel.get_config_value(&config), Some(true));
        assert_eq!(WasmFeature::Simd.get_config_value(&config), None);
        assert_eq!(WasmFeature::Gc.get_config_value(&config), Some(false));
    }

    #[test]
    fn test_feature_and_group_descriptions() {
        assert_eq!(WasmFeature::ComponentModel.description(), "WebAssembly Component Model");
        assert_eq!(WasmFeatureGroup::ComponentModel.description(), "Standard Component Model with WIT interfaces");
    }
}
