// arcella/arcella-wasmtime/src/engine.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::Path;

use wasmtime::{ResourceLimiter, Store};


use arcella_engine::{
    ArcellaEngineError,
    ArcellaEngineResult,
    SupportedFeature, 
    WasmEngine, 
    WasmEngineCapabilities, 
    WasmEngineConfig, 
    WasmFeature,
};
use arcella_types::{
    manifest::ComponentManifest,
};

use super::{
    manifest::component_manifest_from_wasm,
};

const WASM_PAGE_SIZE: usize = 65536;

struct MemoryLimiter {
    max_memory_pages: u32,
    current_memory_pages: u32,
}

impl ResourceLimiter for MemoryLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        let desired_pages = desired / WASM_PAGE_SIZE;

        if desired_pages <= self.max_memory_pages as usize {
            self.current_memory_pages = desired_pages as u32;
            Ok(true)
        } else {
            Ok(false) 
        }
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(true) 
    }
}


/// A WebAssembly engine adapter for Wasmtime.
///
/// This struct encapsulates all Wasmtime-specific logic,
/// allowing `arcella-core` to remain engine-agnostic.
pub struct WasmtimeEngine {
    engine: wasmtime::Engine,
    config: WasmEngineConfig,
}

impl WasmtimeEngine {
    /// Creates a new Wasmtime engine from the given Arcella-agnostic config.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The `WasmEngineConfig` specifies a memory limit outside reasonable bounds.
    /// - Wasmtime fails to initialize the engine (e.g., due to an unsupported feature).
    pub fn new(config: WasmEngineConfig) -> ArcellaEngineResult<Self> {
        // Validate the config first (memory limits, etc.)
        config.validate()?;

        let mut wasmtime_config = wasmtime::Config::new();

        // --- Map Arcella features to Wasmtime config ---

        // Component Model (required for Arcella)
        if config
            .enable_component_model
            .unwrap_or(true) // Default to enabled if not specified
        {
            wasmtime_config.wasm_component_model(true);
        }


        // Reference Types
        if let Some(enable) = config.enable_reference_types {
            wasmtime_config.wasm_reference_types(enable);
        }

        // Bulk Memory
        if let Some(enable) = config.enable_bulk_memory {
            wasmtime_config.wasm_bulk_memory(enable);
        }

        // SIMD
        if let Some(enable) = config.enable_simd {
            wasmtime_config.wasm_simd(enable);
        }

        // Multi-value (almost always required)
        if let Some(enable) = config.enable_multi_value {
            wasmtime_config.wasm_multi_value(enable);
        }

        // Threading & Shared Memory
        if let Some(enable) = config.enable_threads {
            wasmtime_config.wasm_threads(enable);
            if enable {
                wasmtime_config.parallel_compilation(true);
            }
        }

        // Tail calls
        if let Some(enable) = config.enable_tail_call {
            wasmtime_config.wasm_tail_call(enable);
        }

        // Function References
        if let Some(enable) = config.enable_function_references {
            wasmtime_config.wasm_function_references(enable);
        }

        // Garbage Collection (experimental)
        if let Some(enable) = config.enable_gc {
            wasmtime_config.wasm_gc(enable);
        }

        // Always enable async support for the main thread components
        wasmtime_config.async_support(true);

        // Create the engine
        let engine = wasmtime::Engine::new(&wasmtime_config)
            .map_err(|e| ArcellaEngineError::EngineError(e.to_string()))?;

        Ok(Self { engine, config })
    }
}


impl WasmEngine for WasmtimeEngine {
    async fn inspect_component(&self, wasm_path: &Path) -> ArcellaEngineResult<ComponentManifest> {
        component_manifest_from_wasm(&self.engine, wasm_path)
    }

    fn name(&self) -> &'static str { "wasmtime" }
    fn version(&self) -> &'static str { "40.0.0" }
    fn api_version(&self) -> u32 { 1 }
}


impl WasmEngineCapabilities for WasmtimeEngine {
    fn supported_features(&self) -> Vec<SupportedFeature> {
        vec![
            SupportedFeature {
                arcella_name: WasmFeature::ComponentModel.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_component_model".to_string(),
                notes: None,
            },
            SupportedFeature {
                arcella_name: WasmFeature::ReferenceTypes.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_reference_types".to_string(),
                notes: None,
            },
            SupportedFeature {
                arcella_name: WasmFeature::BulkMemory.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_bulk_memory".to_string(),
                notes: None,
            },
            SupportedFeature {
                arcella_name: WasmFeature::Simd.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_simd".to_string(),
                notes: Some("x86_64/aarch64 only".to_string()),
            },
            SupportedFeature {
                arcella_name: WasmFeature::MultiValue.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_multi_value".to_string(),
                notes: None,
            },
            SupportedFeature {
                arcella_name: WasmFeature::Threads.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_threads".to_string(),
                notes: Some("requires shared memory".to_string()),
            },
            SupportedFeature {
                arcella_name: WasmFeature::Gc.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_gc".to_string(),
                notes: Some("experimental".to_string()),
            },
            SupportedFeature {
                arcella_name: WasmFeature::TailCall.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_tail_call".to_string(),
                notes: None,
            },
            SupportedFeature {
                arcella_name: WasmFeature::FunctionReferences.arcella_name(),
                requested: false,
                supported: true,
                engine_specific_name: "wasm_function_references".to_string(),
                notes: None,
            },
        ]
    }
}