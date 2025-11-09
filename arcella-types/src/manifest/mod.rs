// arcella/arcella-types/src/manifest/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{
    ArcellaTypeError,
    Result,
};

use crate::spec::ComponentItemSpec;

mod interface_list;
pub use interface_list::*;

/// A portable, human-readable descriptor of a WebAssembly component.
///
/// The manifest captures **what a component is**, **what it provides**, and **what it needs** —
/// independently of any specific runtime. It serves three key purposes:
///
/// 1. **Identity**: `name@version` uniquely identifies the component.
/// 2. **Contract**: `imports` and `exports` define its interface boundary (like a WIT package).
/// 3. **Intent**: `capabilities` express environmental requirements (WASI, FS, network, etc.).
///
/// This structure is used:
/// - In `component.toml` for human-authored configs (simple array form),
/// - In internal state snapshots (detailed object form),
/// - During linking, validation, and deployment.
///
/// Because it supports **dual-format input** (via `InterfaceList`), users can write:
/// ```toml
/// imports = ["wasi:cli/stdio@0.2"]
/// ```
/// while tools store:
/// ```json
/// { "imports": { "wasi:cli/stdio@0.2": { "instance": { "exports": { ... } } } } }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComponentManifest {
    /// Human-readable identifier of the component (e.g., `"http-logger"`).
    ///
    /// Must:
    /// - Be non-empty,
    /// - Contain only ASCII letters, digits, hyphens (`-`), and underscores (`_`),
    /// - Be unique within its versioned namespace.
    ///
    /// This is **not** a machine key — use `id()` for that.
    pub name: String,

    /// Semantic version of the component (e.g., `"0.1.0"` or `"1.2.3+edge"`).
    ///
    /// Used for:
    /// - Dependency resolution,
    /// - Hot-reloading (same name, newer version),
    /// - Canonical component identity (`name@version`).
    pub version: String,

    /// Optional short description for documentation or tooling.
    #[serde(default)]
    pub description: Option<String>,

    /// Interfaces this component **provides** to others.
    ///
    /// Each key must be a valid WIT-style interface name:
    /// - With version: `"logger:log@1.0"`
    /// - Without version: `"my:custom"`
    ///
    /// Values are structured interface specs (e.g., `ComponentInstance` trees).
    /// When loaded from a simple config (e.g., TOML array), they default to `Unknown`.
    #[serde(default)]
    pub exports: InterfaceList,

    /// Interfaces this component **requires** from its environment.
    ///
    /// Same format as `exports`. These must be satisfied at link time by:
    /// - The runtime (e.g., `wasi:cli/stdio`),
    /// - Other deployed components (e.g., `"auth:validator@1.0"`).
    #[serde(default)]
    pub imports: InterfaceList,

    /// Runtime capabilities and resource requirements.
    ///
    /// Used by the Arcella executor to:
    /// - Grant minimal required permissions,
    /// - Enforce sandboxing,
    /// - Allocate resources safely.
    #[serde(default)]
    pub capabilities: ComponentCapabilities,

    // Future: metadata (authors, license), annotations, tags, etc.
}

impl ComponentManifest {
    /// Returns the **canonical component identifier**: `name@version`.
    ///
    /// This string is:
    /// - Unique within a single runtime instance,
    /// - Validated via `validate_module_id()`,
    /// - Used internally for registry lookups, deployment, and linking.
    ///
    /// Example: `"my-logger@1.2.0+debug"`.
    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }

    /// Validates that a string is a well-formed canonical ID (`name@version`).
    ///
    /// Format: `^[a-zA-Z0-9_-]+@\d+\.\d+\.\d+([+-][a-zA-Z0-9.-]+)?$`
    ///
    /// Examples:
    /// - ✅ `"comp@1.0.0"`, `"edge-mod@0.1.0+alpha"`
    /// - ❌ `"comp v1"`, `"mod@1.0"`
    pub fn validate_module_id(id: &str) -> bool {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"^[a-zA-Z0-9_-]+@\d+\.\d+\.\d+([+-][a-zA-Z0-9.-]+)?$").unwrap()
        });
        re.is_match(id)
    }

    /// Validates component `name` format.
    ///
    /// Allowed: ASCII letters, digits, `_`, `-`. No dots, spaces, or Unicode.
    pub fn validate_name_format(name: &str) -> bool {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap()
        });
        re.is_match(name)
    }

    /// Validates `version` as simplified SemVer (major.minor.patch[+build]).
    ///
    /// Pre-release (`-alpha`) is currently **not supported** to reduce complexity.
    /// Build metadata (`+debug`) is allowed.
    pub fn validate_version_format(version: &str) -> bool {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"^\d+\.\d+\.\d+([+-][a-zA-Z0-9.-]+)?$").unwrap()
        });
        re.is_match(version)
    }

    /// Checks if a string matches the expected WIT interface reference format.
    ///
    /// Two forms are accepted:
    /// - **With version**: `namespace:interface@version` (e.g., `wasi:http@0.2.0`)
    /// - **Without version**: `namespace:interface` (e.g., `my:custom`)
    ///
    /// Interface part may contain `/` for nested paths (e.g., `wasi:cli/stdio`).
    pub fn validate_interface_format(s: &str) -> bool {
        static RE_WITH_VERSION: OnceLock<Regex> = OnceLock::new();
        static RE_WITHOUT_VERSION: OnceLock<Regex> = OnceLock::new();
        
        let re1 = RE_WITH_VERSION.get_or_init(|| {
            Regex::new(r"^[a-zA-Z0-9_-]+:[a-zA-Z0-9_/-]+@[a-zA-Z0-9.+_-]+$").unwrap()
        });
        let re2 = RE_WITHOUT_VERSION.get_or_init(|| {
            Regex::new(r"^[a-zA-Z0-9_-]+:[a-zA-Z0-9_/-]+$").unwrap()
        });
        
        re1.is_match(s) || re2.is_match(s)
    }
    
    /// Validates the semantic correctness of the entire manifest.
    ///
    /// Checks:
    /// - Non-empty and valid `name` and `version`,
    /// - Correct format of all interface keys in `imports` and `exports`.
    ///
    /// Does **not** validate:
    /// - Existence of interface specs (that’s a linking concern),
    /// - Capability feasibility (runtime responsibility).
    ///
    /// Returns `Ok(())` if valid, or a descriptive error otherwise.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(ArcellaTypeError::Manifest("Component name must not be empty".into()));
        }
        if self.version.is_empty() {
            return Err(ArcellaTypeError::Manifest("Component version must not be empty".into()));
        }

        if !Self::validate_name_format(&self.name) {
            return Err(ArcellaTypeError::Manifest(
                "Component name must contain only alphanumeric characters, hyphens, and underscores".into()
            ));
        }

        if !Self::validate_version_format(&self.version) {
            return Err(ArcellaTypeError::Manifest(
                "Component version must follow semantic versioning format (e.g., 0.1.0)".into()
            ));
        }

        for key in self.imports.keys() {
            if !Self::validate_interface_format(key) {
                return Err(ArcellaTypeError::Manifest(
                    format!("Invalid import interface format: {}", key)
                ));
            }
        }
        for key in self.exports.keys() {
            if !Self::validate_interface_format(key) {
                return Err(ArcellaTypeError::Manifest(
                    format!("Invalid export interface format: {}", key)
                ));
            }
        }        

        Ok(())
    }
}

/// Runtime capabilities and environmental requirements of a component.
///
/// This struct enables **least-privilege sandboxing**: the executor grants only what is declared.
/// All fields are **opt-in** — an empty `ComponentCapabilities` means "no special needs".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComponentCapabilities {
    /// Required WASI preview2 interfaces (e.g., `["wasi:cli/stdio", "wasi:random"]`).
    #[serde(default)]
    pub wasi: Vec<String>,
    
    /// Filesystem paths the component needs to access (e.g., `["/logs", "/config"]`).
    ///
    /// Paths are virtualized; actual mapping is runtime-specific.
    #[serde(default)]
    pub filesystem: Vec<String>,
    
    /// Network access patterns (e.g., `["tcp:localhost:8080", "udp:example.com:53"]`).
    ///
    /// Format is not yet standardized — currently treated as opaque strings.
    #[serde(default)]
    pub network: Vec<String>,
    
    /// Required environment variables (e.g., `["DATABASE_URL", "DEBUG"]`).
    #[serde(default)]
    pub environment: Vec<String>,

    /// CPU and memory resource limits.
    #[serde(default)]
    pub resources: ComponentResources,

    /// Security and trusted execution requirements.
    #[serde(default)]
    pub security: ComponentSecurity,
}

/// Resource constraints for sandboxing and QoS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComponentResources {
    /// Maximum memory in bytes (e.g., `67108864` = 64 MiB).
    ///
    /// If `None`, the runtime applies a default or unlimited policy.
    pub memory_max: Option<u64>,

    /// Relative CPU weight (Linux CFS shares equivalent).
    ///
    /// Higher values get more CPU time during contention.
    pub cpu_shares: Option<u32>,
}

/// Security and isolation requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComponentSecurity {
    /// Whether the component **requires** execution inside a TEE (e.g., SGX, TrustZone).
    ///
    /// If `true` and TEE is unavailable, deployment must fail.
    pub requires_tee: bool,

    /// Whitelist of allowed system calls (if runtime supports syscall filtering).
    ///
    /// Empty list = no restriction (or unsupported by runtime).
    pub allowed_syscalls: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_module_id() {
        assert!(ComponentManifest::validate_module_id("my-comp@1.0.0"));
        assert!(ComponentManifest::validate_module_id("edge@0.1.0+debug"));
        assert!(!ComponentManifest::validate_module_id("my comp@1.0.0")); // space
        assert!(!ComponentManifest::validate_module_id("mod@1.0"));       // missing patch
    }

    #[test]
    fn test_component_manifest_json_roundtrip() {
        let mut manifest = ComponentManifest::default();
        manifest.name = "test".to_string();
        manifest.version = "1.0.0".to_string();
        manifest.exports.insert("logger:log@1.0".to_string(), ComponentItemSpec::Unknown { debug: None });
        manifest.imports.insert("wasi:http@0.2.0".to_string(), ComponentItemSpec::Unknown { debug: None });

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        eprintln!("JSON:\n{}", json);

        let restored: ComponentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, restored);
    }

    #[test]
    fn test_invalid_interface_format_rejected() {
        let mut manifest = ComponentManifest {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            ..Default::default()
        };
        manifest.imports.insert("bad::interface".to_string(), ComponentItemSpec::Unknown { debug: None });
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_component_manifest_from_array_compat() {
        // Simulate TOML-style config: interfaces as arrays
        let json = r#"
        {
            "name": "test",
            "version": "1.0.0",
            "imports": ["wasi:cli@0.2.0"],
            "exports": ["foo:bar@1.0"]
        }
        "#;

        let manifest: ComponentManifest = serde_json::from_str(json).unwrap();

        assert!(manifest.imports.contains_key("wasi:cli@0.2.0"));
        assert!(manifest.exports.contains_key("foo:bar@1.0"));
        assert_eq!(
            manifest.imports.get("wasi:cli@0.2.0"),
            Some(&ComponentItemSpec::Unknown { debug: None })
        );
    }
}
