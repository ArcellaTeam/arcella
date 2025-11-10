// arcella/arcella-wasmtime/src/manifest.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use wasmtime::{Engine, component::Component};

use arcella_types::{
    manifest::{
        ComponentManifest,
        ComponentCapabilities,
        InterfaceList,
        ModuleId
    },
    spec::ComponentItemSpec,
};

use crate::{
    ArcellaWasmtimeError,
    ArcellaWasmtimeResult,
    from_wasmtime::ComponentItemSpecExt,
};

/// Extracts component metadata directly from a WebAssembly Component binary.
///
/// This function:
/// - Only works with **WebAssembly Components** (not core Wasm or WASI preview1 modules).
/// - Extracts imports and exports in the format `namespace:interface`.
/// - Does **not** include version (`@x.y`) — this must be provided via `component.toml`
///   or inferred from file naming convention if needed later.
/// - Requires a valid `name` and `version` — since they are not stored in Wasm,
///   this function infers them from the filename (e.g., `http-logger@0.1.0.wasm`).
///
/// For MVP v0.2.3, we assume that if `component.toml` is missing,
/// the filename encodes `name@version`.
pub fn component_manifest_from_wasm(engine: &Engine, wasm_path: &Path) -> ArcellaWasmtimeResult<ComponentManifest> {
    if !wasm_path.exists() {
        return Err(ArcellaWasmtimeError::IoWithPath {
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
            path: wasm_path.into(),
        });
    }

    let file_stem = wasm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ArcellaWasmtimeError::Manifest("Invalid .wasm filename".into()))?;

    let module_id = ModuleId::from_str(file_stem)?;

    let component = Component::from_file(engine, wasm_path)
        .map_err(ArcellaWasmtimeError::Wasmtime)?;

    let component_type = component.component_type();

    let exports: HashMap<String, ComponentItemSpec> = component_type
        .exports(engine)
        .map(|(name, item)| {
            let spec = item.to_spec(engine).unwrap_or_else(|e| {
                ComponentItemSpec::Unknown {
                    debug: Some(format!("Export '{}': {:?}", name, e)),
                }
            });
            (name.into(), spec)
        })
        .collect();

    let imports: HashMap<String, ComponentItemSpec> = component_type
        .imports(engine)
        .map(|(name, item)| {
            let spec = item.to_spec(engine).unwrap_or_else(|e| {
                ComponentItemSpec::Unknown {
                    debug: Some(format!("Import '{}': {:?}", name, e)),
                }
            });
            (name.into(), spec)
        })
        .collect();

    let manifest = ComponentManifest {
        id: module_id,
        description: None,
        exports: InterfaceList::from(exports),
        imports: InterfaceList::from(imports),
        capabilities: ComponentCapabilities::default(),
    };

    manifest.validate()?;
    Ok(manifest)
}
