# `arcella-engine`

> **WebAssembly Engine-Agnostic Interface for Modular, Secure, and Portable Runtimes**

`arcella-engine` is a **core abstraction layer** that enables the [Arcella](https://github.com/ArcellaTeam/arcella) modular WebAssembly platform to work with **multiple WebAssembly runtimes** (e.g., Wasmtime, Wazero, Wasmer) through a **unified, safe, and adaptive API**.

[![crates.io](https://img.shields.io/crates/v/arcella-engine.svg)](https://crates.io/crates/arcella-engine)
[![docs.rs](https://img.shields.io/docsrs/arcella-engine.svg)](https://docs.rs/arcella-engine)
[![license: Apache-2.0/MIT](https://img.shields.io/badge/license-Apache%202.0%20%7C%20MIT-blue.svg)](https://github.com/ArcellaTeam/arcella)

Part of the [**Arcella**](https://github.com/ArcellaTeam) ecosystem.

---

## 🚀 Features

- ✅ **Engine-agnostic configuration**  
- ✅ **Feature detection and graceful degradation**  
- ✅ **Predefined usage profiles** (WASI, Component Model, Secure, High-Performance, etc.)  
- ✅ **Detailed compatibility diagnostics**  
- ✅ **Safe introspection of `.wasm` components**

This crate is designed for **platform builders** who need to support diverse WebAssembly workloads — from tiny embedded WASI modules to complex, typed Component Model applications — without being locked into a single engine.

---

## 🔑 Key Concepts

### 1. **`WasmFeature`**  
A curated registry of WebAssembly capabilities:
- `ComponentModel`, `ReferenceTypes`, `SIMD`, `Threads`, `GC`, and more.

### 2. **`WasmFeatureGroup` (Profiles)**  
Pre-configured sets of features for common scenarios:
- `WasiOnly` – legacy/core modules  
- `ComponentModel` – standard Arcella components (default)  
- `Secure` – hardened, no threads/JIT  
- `HighPerformance` – SIMD + threads + bulk memory  
- `EmbeddedMinimal` – ultra-light for constrained devices

### 3. **`WasmEngineConfig`**  
Flexible configuration with **three-state logic** (`Some(true)`, `Some(false)`, `None`) to allow engines to make smart defaults.

### 4. **`WasmEngineCompatibilityReport`**  
Instead of failing, Arcella **reports** compatibility gaps:
- Warns about missing non-critical features
- Flags **critical gaps** (e.g., Component Model requested but unsupported)
- Always remains **runnable** in degraded mode

### 5. **`WasmEngine` Trait**  
The integration point for any WebAssembly runtime:
```rust
#[async_trait]
pub trait WasmEngine: WasmEngineCapabilities + Send + Sync {
    async fn inspect_component(&self, wasm_path: &Path) -> Result<ComponentManifest>;
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn api_version(&self) -> u32;
}
```

---

## 🚀 Quick Start

### Add to your `Cargo.toml`
```toml
[dependencies]
arcella-engine = "0.1"
```

### Configure an engine profile
```rust
use arcella_engine::{WasmEngineConfig, WasmFeatureGroup};

let config = WasmEngineConfig::default()
    .with_profile(WasmFeatureGroup::ComponentModel)
    .enable_threads(false); // override profile

config.validate()?; // optional
```

### Check compatibility before deployment
```rust
let engine = MyWasmtimeAdapter::new(config);
let report = engine.compatibility_report(&config);

if report.has_critical_gaps() {
    log::warn!("Component Model not supported – falling back to WASI mode");
}

// Proceed even if some features are missing
assert!(report.is_runnable());
```

---

## 🧪 Example: Mock Engine (for testing)

The crate includes utilities to **mock engine capabilities** for integration tests:

```rust
let mock = MockWasmEngine::new(); // supports Component Model, but not SIMD or GC
let report = mock.compatibility_report(&config);
assert!(report.warnings.iter().any(|w| w.contains("simd")));
```

---

## 🧩 Integration with Arcella Ecosystem

- **`arcella-core`**: Uses this crate to validate and deploy components  
- **`arcella-wasmtime`**: Implements `WasmEngine` for Wasmtime (reference adapter)  
- **`arcella-types`**: Shared types like `ComponentManifest`

> 🔗 This crate **does not depend on any WebAssembly engine directly** — it defines the *interface*. Actual engine bindings live in separate adapter crates.

---

## 📜 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `arcella-engine` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
