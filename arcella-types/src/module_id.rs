// arcella-types/src/module_id.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

use crate::error::{ArcellaError, ArcellaResult};

/// Unique identifier for an installed module, in the form `name@version`.
///
/// - `name`: must match `^[a-zA-Z0-9_-]+$`
/// - `version`: must be a **strict** semantic version `x.y.z` (three numeric parts, no leading zeros)
///
/// Build metadata (`+...`) and pre-release (`-...`) are **not supported** to ensure uniqueness
/// and simplify dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ModuleId {
    pub name: String,
    pub version: String,
}

impl ModuleId {
    /// Creates a new `ModuleId` after validating name and version.
    pub fn new(name: String, version: String) -> ArcellaResult<Self> {
        if !is_valid_name(&name) {
            return Err(ArcellaError::InvalidModuleIdName(name));
        }
        if !is_valid_simple_version(&version) {
            return Err(ArcellaError::InvalidModuleIdVersion(version));
        }
        Ok(ModuleId { name, version })
    }

    /// Returns the string representation: `name@version`.
    pub fn to_string(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

impl FromStr for ModuleId {
    type Err = ArcellaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            // name: 1+ alphanum, underscore, dash
            // version: x.y.z where x,y,z are numbers (no leading zeros except for "0")
            Regex::new(r"^(?P<name>[a-zA-Z0-9_-]+)@(?P<version>\d+\.\d+\.\d+)$").unwrap()
        });

        re.captures(s).ok_or_else(|| ArcellaError::InvalidModuleIdFormat(s.to_string()))
            .and_then(|caps| {
                let name = caps.name("name").unwrap().as_str().to_string();
                let version = caps.name("version").unwrap().as_str().to_string();
                ModuleId::new(name, version)
            })
    }
}

impl<'de> Deserialize<'de> for ModuleId {
    /// Deserializes from:
    /// - String: `"name@version"`
    /// - Struct: `{ "name": "...", "version": "..." }`
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ModuleIdHelper {
            String(String),
            Struct { name: String, version: String },
        }

        let helper = ModuleIdHelper::deserialize(deserializer)?;
        match helper {
            ModuleIdHelper::String(s) => {
                ModuleId::from_str(&s).map_err(serde::de::Error::custom)
            }
            ModuleIdHelper::Struct { name, version } => {
                ModuleId::new(name, version).map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Validates component/module name.
///
/// Must be non-empty and contain only ASCII letters, digits, underscores, or hyphens.
pub fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Validates a simple version string like "1.2.3".
///
/// - Must have exactly three dot-separated numeric parts.
/// - Each part must be a non-negative integer.
/// - No leading zeros (except for "0" itself).
pub fn is_valid_simple_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return false;
    }

    for part in parts {
        if part.is_empty() {
            return false;
        }
        // Check for leading zeros (e.g., "01" is invalid, but "0" is OK)
        if part.len() > 1 && part.starts_with('0') {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_module_ids() {
        let cases = vec![
            "logger@1.0.0",
            "http_handler@2.10.5",
            "a@0.0.1",
            "my_mod_v2@1.2.3",
            "test-1@9.9.9",
        ];

        for case in cases {
            let id = ModuleId::from_str(case).expect(&format!("Failed to parse: {}", case));
            assert_eq!(id.to_string(), case);
        }
    }

    #[test]
    fn test_invalid_names() {
        let invalid_names = vec![
            "",                     // empty
            "logger@",              // no version
            "@1.0.0",               // no name
            "logger@1.0",           // bad version format (only two parts)
            "logger@1.0.0.0",       // too many parts
            "logger@1.02.0",        // leading zero in minor
            "logger@1.0.00",        // leading zero in patch
            "logger@1.0.0a",        // non-numeric
            "logger with space@1.0.0", // invalid char
            "logger!@1.0.0",        // invalid char
        ];

        for name in invalid_names {
            assert!(ModuleId::from_str(name).is_err(), "Should fail: {}", name);
        }
    }

    #[test]
    fn test_is_valid_simple_version() {
        assert!(is_valid_simple_version("1.0.0"));
        assert!(is_valid_simple_version("0.0.0"));
        assert!(is_valid_simple_version("123.45.6"));

        // Invalid cases
        assert!(!is_valid_simple_version("1.0"));        // too short
        assert!(!is_valid_simple_version("1.0.0.0"));    // too long
        assert!(!is_valid_simple_version("1.02.0"));     // leading zero
        assert!(!is_valid_simple_version("1.0.00"));     // leading zero
        assert!(!is_valid_simple_version("1.a.0"));      // non-digit
        assert!(!is_valid_simple_version("1..0"));       // empty part
        assert!(!is_valid_simple_version(""));           // empty
        assert!(!is_valid_simple_version("01.0.0"));     // leading zero in major
    }

    #[test]
    fn test_is_valid_name() {
        assert!(is_valid_name("a"));
        assert!(is_valid_name("logger"));
        assert!(is_valid_name("http_handler"));
        assert!(is_valid_name("test-123"));
        assert!(is_valid_name("123mod")); // starts with digit — OK

        assert!(!is_valid_name(""));
        assert!(!is_valid_name("logger!"));
        assert!(!is_valid_name("logger with space"));
        assert!(!is_valid_name(&"x".repeat(129))); // too long
    }

    #[test]
    fn test_module_id_deserialize_string() {
        let json = r#""test@1.0.0""#;
        let id: ModuleId = serde_json::from_str(json).unwrap();
        assert_eq!(id.name, "test");
        assert_eq!(id.version, "1.0.0");
    }

    #[test]
    fn test_module_id_deserialize_struct() {
        let json = r#"{"name":"my_mod","version":"2.10.5"}"#;
        let id: ModuleId = serde_json::from_str(json).unwrap();
        assert_eq!(id.name, "my_mod");
        assert_eq!(id.version, "2.10.5");
    }

    #[test]
    fn test_module_id_serialize_always_as_struct() {
        let id = ModuleId::new("test".into(), "1.0.0".into()).unwrap();
        let json = serde_json::to_string(&id).unwrap();
        // Должно быть объектом, не строкой
        assert!(json.starts_with('{'));
        assert!(json.contains(r#""name":"test""#));
        assert!(json.contains(r#""version":"1.0.0""#));
    }

    #[test]
    fn test_module_id_deserialize_invalid_string() {
        let json = r#""bad@@version""#;
        let err = serde_json::from_str::<ModuleId>(json).unwrap_err();
        assert!(err.to_string().contains("Invalid module ID format"));
    }

    #[test]
    fn test_module_id_deserialize_invalid_struct() {
        let json = r#"{"name":"in valid","version":"1.0.0"}"#;
        let err = serde_json::from_str::<ModuleId>(json).unwrap_err();
        assert!(err.to_string().contains("Invalid module ID name"));
    }
}
