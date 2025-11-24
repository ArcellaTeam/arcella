// arcella/arcella-core/src/engine/mod.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Engine-agnostic interface for executing WebAssembly components.
//!
//! Arcella is a platform that **adapts to the capabilities of the engine**, rather than
//! requiring strict feature alignment. This enables:
//! - Running components even on minimal engines (e.g., WASI-only),
//! - Leveraging advanced features (Component Model, GC, etc.) when available,
//! - Smooth migration between engines without rewriting manifests or deployment specs.
//!
//! ## Capability Levels
//!
//! Arcella distinguishes three levels of functionality:
//! - **Level 0**: Core WebAssembly + WASI — basic support for `.wasm` modules
//!   without type-safe interfaces. Suitable for simple services.
//! - **Level 1**: Component Model with WIT — components declare typed interfaces in WIT,
//!   and Arcella ensures type-safe inter-component communication.
//! - **Level 2+**: Advanced features (GC, SIMD, function references, etc.),
//!   available only in modern engines (e.g., Wasmtime ≥15).
//!
//! ## Usage
//!
//! ### Configuration
//! ```rust
//! let config = WasmEngineConfig {
//!     enable_component_model: Some(true),  // I need WIT support
//!     enable_simd: None,                   // enable if the engine supports it
//!     enable_gc: Some(false),              // explicitly disabled
//!     ..Default::default()
//! };
//! ```
//!
//! ### Compatibility Report
//! ```rust
//! let engine = WasmtimeEngine::new(&config)?;
//! let report = engine.compatibility_report(&config);
//! for warning in &report.warnings {
//!     tracing::warn!("{}", warning);
//! }
//! if report.has_critical_gaps() {
//!     // e.g., Component Model was requested but is not supported
//!     return Err(...);
//! }
//! ```
//!
//! ### Component Introspection
//! ```rust
//! let manifest = engine.inspect_component(Path::new("my.wasm"))?;
//! match manifest.component_type {
//!     ComponentType::CoreWasi => { /* run as WASI module */ }
//!     ComponentType::ComponentModel => { /* use WIT adapters */ }
//! }
//! ```
//!
//! ## Implementing Support for a New Engine
//!
//! To add support for a new engine (e.g., `wazero`):
//! 1. Create a new crate, e.g., `arcella-wazero`.
//! 2. Implement `WasmEngineCapabilities::supported_features()` with accurate details.
//! 3. Implement `WasmEngine::inspect_component()` with fallback to core modules.
//! 4. Ensure `inspect_component` works even if Component Model is unsupported.
//!
//! This design guarantees that Arcella remains **flexible, secure, and portable**.

use std::path::Path;

use arcella_types::manifest::ComponentManifest;

use crate::ArcellaResult;

/// Configuration of WebAssembly engine capabilities, independent of implementation.
///
/// All fields are `Option<bool>` to enable **flexibility**:
/// - `Some(true)` — the feature is **required**; an error occurs if unsupported,
/// - `Some(false)` — the feature is **explicitly disabled**,
/// - `None` — the feature is **optional**; the engine may enable it if supported.
///
/// ### Example
/// ```rust
/// WasmEngineConfig {
///     enable_component_model: Some(true),  // critical for WIT-based components
///     enable_simd: None,                   // enable on x86_64, ignore on RISC-V
///     enable_gc: Some(false),              // never use
///     ..Default::default()
/// }
/// ```
#[derive(Debug, Clone)]
pub struct WasmEngineConfig {
    /// Maximum number of 64 KiB memory pages (default: 16,384 = 1 GiB).
    pub max_memory_pages: Option<u32>,

    /// Bulk memory operations: `memory.copy`, `memory.fill`, `data.drop`, etc.
    pub enable_bulk_memory: Option<bool>,

    /// Reference types support: `externref`, `funcref` (required for Component Model).
    pub enable_reference_types: Option<bool>,

    /// 128-bit SIMD instructions (platform-dependent).
    pub enable_simd: Option<bool>,

    /// Functions with multiple return values.
    pub enable_multi_value: Option<bool>,

    /// [Component Model](https://component-model.bytecodealliance.org/) — foundation for type-safe interfaces.
    pub enable_component_model: Option<bool>,

    /// Threading support: shared memory and atomic operations.
    pub enable_threads: Option<bool>,

    /// Tail calls (`return_call`, `return_call_indirect`).
    pub enable_tail_call: Option<bool>,

    /// Function references (part of Wasm GC proposal, stage 1).
    pub enable_function_references: Option<bool>,

    /// Full WebAssembly Garbage Collection (Wasm GC) support.
    pub enable_gc: Option<bool>,
}

impl Default for WasmEngineConfig {
    fn default() -> Self {
        Self {
            max_memory_pages: Some(16_384), // 1 GiB
            enable_bulk_memory: Some(true),
            enable_reference_types: Some(true),
            enable_simd: None,        // let engine decide based on platform
            enable_multi_value: Some(true),
            enable_component_model: Some(true),
            enable_threads: None,
            enable_tail_call: None,
            enable_function_references: None,
            enable_gc: None,
        }
    }
}

/// Detailed information about the support status of a specific WebAssembly feature.
#[derive(Debug, Clone)]
pub struct SupportedFeature {
    /// The canonical Arcella name of the feature.
    ///
    /// Used in configuration and logs. Examples:
    /// - `"component_model"`
    /// - `"simd"`
    /// - `"gc"`
    pub arcella_name: &'static str,

    /// Whether the feature was explicitly requested as `Some(true)` in `WasmEngineConfig`.
    pub requested: bool,

    /// Whether the engine supports this feature on the current platform.
    pub supported: bool,

    /// Engine-specific name or identifier for the feature.
    ///
    /// Examples:
    /// - `"wasm_component_model"` (Wasmtime)
    /// - `"WithWasmCoreConfig().WithFeatureSIMD()"` (Wazero)
    pub engine_specific_name: String,

    /// Additional notes: limitations, conditions, or version requirements.
    ///
    /// Examples:
    /// - `"only on x86_64 and aarch64"`
    /// - `"requires --experimental-wasm-gc"`
    /// - `"not yet implemented"`
    pub notes: Option<String>,
}

/// Compatibility report between configuration and engine capabilities.
///
/// Contains a **complete list of features** with support status and **human-readable warnings**.
#[derive(Debug, Clone, Default)]
pub struct WasmEngineCompatibilityReport {
    /// All features known to Arcella, annotated with request and support status.
    pub features: Vec<SupportedFeature>,

    /// Human-readable warnings (e.g., for logging or CLI output).
    ///
    /// Example: `"requested feature 'simd' (wasm_simd) is not supported: only on x86_64"`
    pub warnings: Vec<String>,
}

impl WasmEngineCompatibilityReport {
    /// Checks whether any **critically required** features are requested but unsupported.
    ///
    /// Currently, only `component_model` is considered critical, because
    /// WIT-based interfaces — the foundation of Arcella’s modularity —
    /// cannot function without it.
    pub fn has_critical_gaps(&self) -> bool {
        self.features.iter().any(|f| {
            f.arcella_name == "component_model" && f.requested && !f.supported
        })
    }

    /// Returns `true` if the platform can run (even in a degraded mode).
    ///
    /// Arcella always supports **at least WASI**, so this method almost always
    /// returns `true`. Fatal initialization errors are not reflected here.
    pub fn is_runnable(&self) -> bool {
        true
    }
}

/// Trait describing the capabilities of a WebAssembly engine.
///
/// Implementations must:
/// - Return **complete and accurate** feature support information,
/// - Account for **platform constraints** (e.g., SIMD only on x86_64),
/// - Never panic when calling `supported_features()` or `compatibility_report()`.
pub trait WasmEngineCapabilities {
    /// Returns a list of all features the engine **may potentially support**,
    /// annotated with their current support status on this platform.
    ///
    /// This list **must include all features from `WasmEngineConfig`**,
    /// even if `supported = false`.
    fn supported_features(&self) -> Vec<SupportedFeature>;

    /// Compares the given configuration against engine capabilities and returns a report.
    ///
    /// The report includes:
    /// - All features with `requested` and `supported` flags,
    /// - Warnings for requested but unsupported features.
    ///
    /// This method **never returns an error**, even if a feature is unsupported —
    /// adaptation and graceful degradation are core principles of Arcella.
    fn compatibility_report(&self, config: &WasmEngineConfig) -> WasmEngineCompatibilityReport {
        let mut report = WasmEngineCompatibilityReport::default();
        let known_features = self.supported_features();

        let find_feature = |name: &'static str| -> Option<&SupportedFeature> {
            known_features.iter().find(|f| f.arcella_name == name)
        };

        let mut add_feature_to_report = |arc_name: &'static str, config_value: Option<bool>| {
            let requested = config_value.unwrap_or(false);
            if let Some(feat) = find_feature(arc_name) {
                if requested && !feat.supported {
                    report.warnings.push(format!(
                        "requested feature '{}' ({}) is not supported{}",
                        arc_name,
                        feat.engine_specific_name,
                        feat.notes.as_ref().map_or(String::new(), |n| format!(": {}", n))
                    ));
                }
                report.features.push(SupportedFeature {
                    arcella_name: arc_name,
                    requested,
                    supported: feat.supported,
                    engine_specific_name: feat.engine_specific_name.clone(),
                    notes: feat.notes.clone(),
                });
            } else {
                report.warnings.push(format!(
                    "requested feature '{}' is not recognized by engine",
                    arc_name
                ));
                report.features.push(SupportedFeature {
                    arcella_name: arc_name,
                    requested,
                    supported: false,
                    engine_specific_name: "unknown".to_string(),
                    notes: Some("not implemented in this engine".to_string()),
                });
            }
        };

        add_feature_to_report("component_model", config.enable_component_model);
        add_feature_to_report("reference_types", config.enable_reference_types);
        add_feature_to_report("bulk_memory", config.enable_bulk_memory);
        add_feature_to_report("simd", config.enable_simd);
        add_feature_to_report("gc", config.enable_gc);
        add_feature_to_report("multi_value", config.enable_multi_value);
        add_feature_to_report("threads", config.enable_threads);
        add_feature_to_report("tail_call", config.enable_tail_call);
        add_feature_to_report("function_references", config.enable_function_references);

        report
    }
}

/// Primary interface for interacting with a WebAssembly engine.
///
/// Implementations must:
/// - Support **fallback to core modules** when Component Model is unavailable,
/// - Return a `ComponentManifest` suitable for deployment,
/// - Be `Send + Sync` for use in a multi-threaded runtime.
#[allow(async_fn_in_trait)]
pub trait WasmEngine: WasmEngineCapabilities + Send + Sync {
    /// Analyzes a `.wasm` file and constructs a `ComponentManifest`.
    ///
    /// ### Behavior
    /// - If the file is a **Component Model**, extracts WIT interfaces.
    /// - If the file is a **core module**, returns a `CoreWasi` manifest with `wasi:*` imports.
    /// - If the file is corrupted, returns an error.
    ///
    /// ### Guarantees
    /// - Never panics.
    /// - Does not block for extended periods (bounded by I/O and parsing timeouts).
    /// - Works even when `component_model = false`.
    fn inspect_component(&self, wasm_path: &Path) -> ArcellaResult<ComponentManifest>;

    /// Returns a short, stable name of the engine for logging and metrics.
    ///
    /// Examples: `"wasmtime"`, `"wazero"`, `"wasm3"`.
    fn name(&self) -> &'static str;
}