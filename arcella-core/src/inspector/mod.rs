// arcella-core/src/inspector/mod.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::Path;
use std::sync::Arc;

use arcella_engine::{ArcellaEngineResult, WasmEngine};
use arcella_types::manifest::ComponentManifest;

/// A service for inspecting WebAssembly components.
///
/// This struct wraps an engine-agnostic `Arc<dyn WasmEngine>` and provides
/// a clean interface for metadata extraction.
#[derive(Clone)]
pub struct ComponentInspector {
    engine: Arc<dyn WasmEngine>,
}

impl ComponentInspector {
    /// Creates a new inspector with the given engine.
    pub fn new(engine: Arc<dyn WasmEngine>) -> Self {
        Self { engine }
    }

    /// Extracts a `ComponentManifest` from a `.wasm` file.
    ///
    /// This method:
    /// - delegates to the underlying engine via `inspect_component`;
    /// - ensures the path exists (optional, but improves UX);
    /// - returns a fully validated manifest.
    ///
    /// # Errors
    /// Returns an error if:
    /// - the file does not exist,
    /// - the engine fails to parse the file,
    /// - the resulting manifest is invalid.
    pub async fn inspect_wasm(&self, wasm_path: &Path) -> ArcellaEngineResult<ComponentManifest> {
        // Optional: early existence check (engines may do this too, but we provide clearer context)
        if !wasm_path.exists() {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound)
                .into());
        }

        let manifest = self.engine.inspect_component(wasm_path).await?;
        manifest.validate().map_err(|e| e.into())?;
        Ok(manifest)
    }
}

