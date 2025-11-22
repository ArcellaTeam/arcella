// arcella/arcella-types/src/manifest.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;
use std::sync::OnceLock;

use crate::{
	ArcellaTypeError, 
	ArcellaTypeResult,
};

use crate::module_id::*;

use crate::interface_list::*;

/// A portable, human-readable descriptor of a WebAssembly component.
///
/// The manifest captures **what a component is**, **what it provides**, and **what it needs** —
/// independently of any specific runtime. It serves three key purposes:
///
/// 1. **Identity**: `id` (`name@version`) uniquely identifies the component.
/// 2. **Contract**: `imports` and `exports` define its interface boundary (like a WIT package).
/// 3. **Intent**: `capabilities` express environmental requirements (WASI, FS, network, etc.).
///
/// This structure supports **three input formats** during deserialization:
/// - String: `"name@version"` (e.g., in deployment specs)
/// - Flat object: `{ "name": "...", "version": "...", ... }` (e.g., in `component.toml`)
/// - Nested object: `{ "id": { "name": "...", "version": "..." }, ... }` (e.g., in JSON state snapshots)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentManifest {
    /// Canonical, validated identifier of the component: `name@version`.
    pub id: ModuleId,

    /// Optional short description for documentation or tooling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
}

// ======================================
// Deserialize supporting THREE formats:
// 1. String:       "name@version"
// 2. Flat object:  { "name": "...", "version": "...", ... }
// 3. Nested object:{ "id": { "name": "...", "version": "..." }, ... }
// ======================================

#[derive(Deserialize)]
#[serde(untagged)]
enum ComponentManifestDeserializeHelper {
    // Format 1: just a string ID
    StringId(String),

    // Format 2: nested with explicit "id"
    Nested {
        id: ModuleId,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        exports: InterfaceList,
        #[serde(default)]
        imports: InterfaceList,
        #[serde(default)]
        capabilities: ComponentCapabilities,
    },

    // Format 3: flat with "name" and "version"
    Flat {
        name: String,
        version: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        exports: InterfaceList,
        #[serde(default)]
        imports: InterfaceList,
        #[serde(default)]
        capabilities: ComponentCapabilities,
    },
}

impl<'de> Deserialize<'de> for ComponentManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match ComponentManifestDeserializeHelper::deserialize(deserializer)? {
            // Format 1: string → only id, rest default
            ComponentManifestDeserializeHelper::StringId(s) => {
                let id = ModuleId::from_str(&s).map_err(serde::de::Error::custom)?;
                Ok(ComponentManifest {
                    id,
                    description: None,
                    exports: InterfaceList::default(),
                    imports: InterfaceList::default(),
                    capabilities: ComponentCapabilities::default(),
                })
            }

            // Format 2: nested object
            ComponentManifestDeserializeHelper::Nested {
                id,
                description,
                exports,
                imports,
                capabilities,
            } => Ok(ComponentManifest {
                id,
                description,
                exports,
                imports,
                capabilities,
            }),

            // Format 3: flat object (TOML-style)
            ComponentManifestDeserializeHelper::Flat {
                name,
                version,
                description,
                exports,
                imports,
                capabilities,
            } => {
                let id = ModuleId::new(name, version).map_err(serde::de::Error::custom)?;
                Ok(ComponentManifest {
                    id,
                    description,
                    exports,
                    imports,
                    capabilities,
                })
            }
        }
    }
}

// ======================================
// Validation and helpers
// ======================================

impl ComponentManifest {
    /// Validates the semantic correctness of the entire manifest.
    ///
    /// Since `id` is a `ModuleId`, `name` and `version` are already valid.
    /// This method only checks interface formats.
    pub fn validate(&self) -> ArcellaTypeResult<()> {
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
}

// ======================================
// Capabilities and Resources
// ======================================

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

// ======================================
// Tests
// ======================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use crate::spec::ComponentItemSpec;

    #[test]
    fn test_component_manifest_deserialize_three_formats() {
        // Format 1: string
        let s = r#""http-logger@0.1.0""#;
        let m1: ComponentManifest = serde_json::from_str(s).unwrap();
        assert_eq!(m1.id.to_string(), "http-logger@0.1.0");
        assert!(m1.description.is_none());
        assert!(m1.imports.is_empty());

        // Format 2: nested object
        let json_nested = r#"
        {
            "id": {
                "name": "web-handler",
                "version": "2.0.0"
            },
            "description": "Handles HTTP",
            "imports": ["wasi:http@0.2.0"]
        }
        "#;
        let m2: ComponentManifest = serde_json::from_str(json_nested).unwrap();
        assert_eq!(m2.id.to_string(), "web-handler@2.0.0");
        assert_eq!(m2.description, Some("Handles HTTP".to_string()));
        assert!(m2.imports.contains_key("wasi:http@0.2.0"));

        // Format 3: flat object (TOML-style)
        let json_flat = r#"
        {
            "name": "auth-service",
            "version": "1.5.0",
            "exports": ["auth:verify@1.0"]
        }
        "#;
        let m3: ComponentManifest = serde_json::from_str(json_flat).unwrap();
        assert_eq!(m3.id.to_string(), "auth-service@1.5.0");
        assert!(m3.exports.contains_key("auth:verify@1.0"));
    }

    #[test]
    fn test_component_manifest_from_toml_style() {
        let toml_input = r#"
            name = "http-logger"
            version = "0.1.0"
            description = "Logs HTTP requests"
            exports = ["logger:log@1.0"]
            imports = ["wasi:http/incoming-handler@0.2.0"]
        "#;

        let manifest: ComponentManifest = toml::from_str(toml_input).unwrap();
        assert_eq!(manifest.id.name, "http-logger");
        assert_eq!(manifest.id.version, "0.1.0");
        assert_eq!(manifest.id.to_string(), "http-logger@0.1.0");
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_invalid_name_rejected() {
        let toml_input = r#"
            name = "invalid name!"
            version = "1.0.0"
        "#;
        let err = toml::from_str::<ComponentManifest>(toml_input).unwrap_err();
        assert!(err.to_string().contains("Invalid module ID name"));
    }

    #[test]
    fn test_invalid_interface_rejected() {
        let mut manifest = ComponentManifest {
            id: ModuleId::new("test".into(), "1.0.0".into()).unwrap(),
            description: None,
            exports: InterfaceList::default(),
            imports: InterfaceList::default(),
            capabilities: ComponentCapabilities::default(),
        };
        manifest.imports.insert("bad::interface".into(), ComponentItemSpec::Unknown { debug: None });
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_json_roundtrip() {
        let manifest = ComponentManifest {
            id: ModuleId::new("test".into(), "1.0.0".into()).unwrap(),
            description: Some("A test component".into()),
            exports: {
                let mut m = InterfaceList::default();
                m.insert("logger:log@1.0".into(), ComponentItemSpec::Unknown { debug: None });
                m
            },
            imports: {
                let mut m = InterfaceList::default();
                m.insert("wasi:http@0.2.0".into(), ComponentItemSpec::Unknown { debug: None });
                m
            },
            capabilities: ComponentCapabilities::default(),
        };

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        eprintln!("JSON:\n{}", json);

        let restored: ComponentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, restored);
    }
}
