# arcella-types

[![Crates.io](https://img.shields.io/crates/v/arcella-types)](https://crates.io/crates/arcella-types)
[![Docs.rs](https://img.shields.io/docsrs/arcella-types)](https://docs.rs/arcella-types)
[![License](https://img.shields.io/crates/l/arcella-types)](./LICENSE-APACHE)

**Core data types and manifests for the Arcella WebAssembly runtime.**

This crate defines the **shared contract** used across the [Arcella](https://github.com/your-org/arcella) platform — a modular, secure runtime for WebAssembly Component Model and WASI applications.

It is designed to be:
- ✅ **Pure data**: no runtime logic, no I/O, no external engine dependencies.
- ✅ **Serializable**: full `serde` support for JSON/TOML.
- ✅ **Validated**: strict parsing of module IDs, interfaces, and manifests.
- ✅ **Reusable**: useful for CLI tools, IDE extensions, package managers, and custom integrations.

---

## 📦 Key Types

| Type | Purpose |
|------|--------|
| `ModuleId` | Canonical `name@version` identifier (e.g., `http-logger@0.1.0`) |
| `ComponentManifest` | Describes a component’s identity, interfaces (`imports`/`exports`), and capabilities |
| `InterfaceList` | Dual-format (`["iface"]` or `{"iface": {...}}`) interface declaration |
| `ComponentItemSpec` | Typed representation of WIT items (functions, instances, components, etc.) |
| `ConfigData` | Hierarchical configuration with dot-separated keys (`arcella.log.level`) |
| `Value` | Universal dynamic value type (like `serde_json::Value`, but tailored for Arcella) |

---

## 🚀 Example: Parsing a Component Manifest

```rust
use arcella_types::manifest::ComponentManifest;

let toml = r#"
    name = "http-logger"
    version = "0.1.0"
    description = "Logs HTTP requests"
    exports = ["logger:log@1.0"]
    imports = ["wasi:http/incoming-handler@0.2.0"]
"#;

let manifest: ComponentManifest = toml::from_str(toml)?;
assert_eq!(manifest.id.to_string(), "http-logger@0.1.0");
manifest.validate()?; // Ensures interface format is valid