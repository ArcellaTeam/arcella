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

use std::path::Path;

use arcella_types::manifest::ComponentManifest;

use crate::{ArcellaError, ArcellaResult};

// =============== 1. Typed feature list ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmFeature {
    ComponentModel,
    ReferenceTypes,
    BulkMemory,
    Simd,
    Gc,
    MultiValue,
    Threads,
    TailCall,
    FunctionReferences,
}

impl WasmFeature {
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
// 2. Группы фич (профили)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmFeatureGroup {
    /// Только WASI (core module, без Component Model)
    WasiOnly,

    /// Полная поддержка Component Model (стандартный профиль Arcella)
    ComponentModel,

    /// WASM GC + функциональные ссылки (для продвинутых компонентов)
    GcFull,

    /// Минимальный профиль для embedded-устройств
    EmbeddedMinimal,

    /// Максимальная производительность (SIMD, multi-value, bulk memory)
    HighPerformance,

    /// Максимальная безопасность (без shared memory, threads, JIT)
    Secure,

    /// Экспериментальная фича
    Experimental, 
}

impl WasmFeatureGroup {
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

// =============== 2. Engine configuration ===============

#[derive(Debug, Clone)]
pub struct WasmEngineConfig {
    pub max_memory_pages: Option<u32>,
    pub enable_bulk_memory: Option<bool>,
    pub enable_reference_types: Option<bool>,
    pub enable_simd: Option<bool>,
    pub enable_multi_value: Option<bool>,
    pub enable_component_model: Option<bool>,
    pub enable_threads: Option<bool>,
    pub enable_tail_call: Option<bool>,
    pub enable_function_references: Option<bool>,
    pub enable_gc: Option<bool>,
    pub profile: Option<WasmFeatureGroup>,
}

impl Default for WasmEngineConfig {
    
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
    /// Максимальное разумное количество страниц (4GiB)
    pub const MAX_REASONABLE_PAGES: u32 = 65536;
    
    /// Минимальное количество страниц (64KiB)
    pub const MIN_REASONABLE_PAGES: u32 = 1;

    pub fn with_profile(mut self, profile: WasmFeatureGroup) -> Self {
        self.profile = Some(profile);

        // Включение фич из профиля
        for &feature in profile.features() {
            match feature {
                WasmFeature::ComponentModel if self.enable_component_model.is_none() => {
                    self.enable_component_model = Some(true);
                },
                WasmFeature::ReferenceTypes if self.enable_reference_types.is_none() => {
                    self.enable_reference_types = Some(true);
                },
                WasmFeature::Simd if self.enable_simd.is_none() => {
                    self.enable_simd = Some(true);
                },
                WasmFeature::Gc if self.enable_gc.is_none() => {
                    self.enable_gc = Some(true);
                },
                WasmFeature::Threads if self.enable_threads.is_none() => {
                    self.enable_threads = Some(true);
                },
                WasmFeature::BulkMemory if self.enable_bulk_memory.is_none() => {
                    self.enable_bulk_memory = Some(true);
                },
                WasmFeature::MultiValue if self.enable_multi_value.is_none() => {
                    self.enable_multi_value = Some(true);
                },
                WasmFeature::TailCall if self.enable_tail_call.is_none() => {
                    self.enable_tail_call = Some(true);
                },
                WasmFeature::FunctionReferences if self.enable_function_references.is_none() => {
                    self.enable_function_references = Some(true);
                },
                _ => {}
            }
        }

        self
    }    
    
    pub fn validate(&self) -> Result<(), ArcellaError> {
        if let Some(pages) = self.max_memory_pages {
            if pages < Self::MIN_REASONABLE_PAGES {
                return Err(ArcellaError::MemoryTooSmall(pages));
            }
            if pages > Self::MAX_REASONABLE_PAGES {
                return Err(ArcellaError::MemoryTooLarge(pages));
            }
        }
        Ok(())
    }
}

// =============== 3. Engine compatibility report ===============

#[derive(Debug, Clone)]
pub struct SupportedFeature {
    pub arcella_name: &'static str,
    pub requested: bool,
    pub supported: bool,
    pub engine_specific_name: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WasmEngineCompatibilityReport {
    pub profile: Option<WasmFeatureGroup>,
    pub features: Vec<SupportedFeature>,
    pub warnings: Vec<String>,
}

impl WasmEngineCompatibilityReport {
    pub fn has_critical_gaps(&self) -> bool {
        self.features.iter().any(|f| {
            f.arcella_name == "component_model" && f.requested && !f.supported
        })
    }

    pub fn is_runnable(&self) -> bool {
        true
    }
}


// =============== 4. Trait возможностей ===============

pub trait WasmEngineCapabilities {
    fn supported_features(&self) -> Vec<SupportedFeature>;

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
                // Фича не поддерживается движком вообще
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

// =============== 5. Основной trait движка ===============

#[allow(async_fn_in_trait)]
pub trait WasmEngine: WasmEngineCapabilities + Send + Sync {
    fn inspect_component(&self, wasm_path: &Path) -> ArcellaResult<ComponentManifest>;
    fn name(&self) -> &'static str;
    // Engine version
    fn version(&self) -> &'static str;
    // Arcella Engine API version
    fn api_version(&self) -> u32;
}

// =============================================================================
// Тесты
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Простой мок-движок для тестирования отчётов
    struct MockWasmEngine {
        supported_features_map: HashMap<&'static str, (bool, String, Option<String>)>,
    }

    impl MockWasmEngine {
        fn new() -> Self {
            let mut map = HashMap::new();
            map.insert(
                "component_model", // ← &'static str
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
                    // Ищем в мапе по arcella_name (но мапа теперь по &str)
                    if let Some((supported, engine_name, notes)) = self.supported_features_map.get(arcella_name) {
                        SupportedFeature {
                            arcella_name,
                            requested: false,
                            supported: *supported,
                            engine_specific_name: engine_name.clone(),
                            notes: notes.clone(),
                        }
                    } else {
                        // Неизвестная фича (не должно происходить)
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
        fn inspect_component(&self, _wasm_path: &Path) -> ArcellaResult<ComponentManifest> {
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

        assert_eq!(config.enable_component_model, None); // не входит в WasiOnly
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
            enable_simd: Some(false), // явно отключено
            enable_gc: Some(false),
            ..Default::default()
        };

        let report = engine.compatibility_report(&config);

        assert!(!report.has_critical_gaps());
        assert!(report.is_runnable());

        // Проверим, что SIMD и GC не вызвали предупреждений (они отключены)
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

        // В моке component_model = true → нет критического разрыва
        assert!(!report.has_critical_gaps());

        // Теперь создадим движок, который НЕ поддерживает Component Model
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