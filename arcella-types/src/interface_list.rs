// arcella-types/src/manifest/interface_list.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use flexicon::adaptive::{FromName, NamedMap};

use crate::spec::ComponentItemSpec;

/// `FromName` impl for `ComponentItemSpec` to enable `NamedMap` dual-format input.
impl FromName for ComponentItemSpec {
    fn from_name(_name: &str) -> Self {
        // Matches original behavior: Unknown { debug: None }
        ComponentItemSpec::Unknown { debug: None }
    }
}

/// A wrapper for interface declarations that supports dual-format deserialization:
/// - As JSON object (for persistence and internal state): `{ "iface": { "unknown": {} } }`
/// - As JSON array of strings (for human-authored configs): `["iface"]`
///
/// Implemented as a type alias over `flexicon::adaptive::NamedMap<ComponentItemSpec>`.
///
/// Always stores data as `HashMap<String, ComponentItemSpec>` for O(1) lookup.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InterfaceList(NamedMap<ComponentItemSpec>);

impl InterfaceList {
    /// Creates an empty interface list.
    pub fn new() -> Self {
        Self(NamedMap::new())
    }

    /// Inserts an interface.
    pub fn insert(&mut self, key: String, value: ComponentItemSpec) {
        self.0.insert(key, value);
    }

    /// Returns an iterator over interfaces.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ComponentItemSpec)> {
        self.0.iter()
    }

    /// Returns `true` if no interfaces are present.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes and returns the inner map.
    pub fn into_inner(self) -> HashMap<String, ComponentItemSpec> {
        self.0.into_inner()
    }

    /// Returns a reference to the inner map.
    pub fn as_inner(&self) -> &HashMap<String, ComponentItemSpec> {
        self.0.as_inner()
    }

    /// Returns a mutable reference to the inner map.
    ///
    /// ⚠️ Direct mutation bypasses any future validation.
    pub fn as_inner_mut(&mut self) -> &mut HashMap<String, ComponentItemSpec> {
        self.0.as_inner_mut()
    }
}

// === Deref for seamless HashMap usage ===

impl Deref for InterfaceList {
    type Target = HashMap<String, ComponentItemSpec>;

    fn deref(&self) -> &Self::Target {
        self.as_inner()
    }
}

impl DerefMut for InterfaceList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_inner_mut()
    }
}

// === From conversions ===

impl From<Vec<String>> for InterfaceList {
    fn from(list: Vec<String>) -> Self {
        // Delegate to NamedMap’s From<Vec<String>>
        Self(NamedMap::from(list))
    }
}

impl From<HashMap<String, ComponentItemSpec>> for InterfaceList {
    fn from(map: HashMap<String, ComponentItemSpec>) -> Self {
        Self(NamedMap::from(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_interface_list() {
        let list = InterfaceList::new();
        assert!(list.is_empty());
        assert_eq!(list.0.len(), 0);

        let json = serde_json::to_string(&list).unwrap();
        let restored: InterfaceList = serde_json::from_str(&json).unwrap();
        assert_eq!(list, restored);
    }

    #[test]
    fn test_deserialize_from_valid_object() {
        let input = json!({
            "logger:log@1.0": { "unknown": {} },
            "wasi:http@0.2.0": { "unknown": { "debug": "from registry" } }
        });

        let list: InterfaceList = serde_json::from_value(input).unwrap();
        assert_eq!(list.0.len(), 2);
        assert!(list.0.contains_key("logger:log@1.0"));
        assert!(list.0.contains_key("wasi:http@0.2.0"));

        match &list.0["logger:log@1.0"] {
            ComponentItemSpec::Unknown { debug } => assert!(debug.is_none()),
            _ => panic!("expected Unknown"),
        }

        match &list.0["wasi:http@0.2.0"] {
            ComponentItemSpec::Unknown { debug } => {
                assert_eq!(debug.as_deref(), Some("from registry"));
            }
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn test_deserialize_from_empty_object() {
        let input = json!({});
        let list: InterfaceList = serde_json::from_value(input).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_deserialize_from_valid_array() {
        let input = json!(["iface1", "iface2@1.0"]);
        let list: InterfaceList = serde_json::from_value(input).unwrap();
        assert_eq!(list.0.len(), 2);
        assert!(list.0.contains_key("iface1"));
        assert!(list.0.contains_key("iface2@1.0"));

        for spec in list.0.values() {
            assert!(matches!(spec, ComponentItemSpec::Unknown { debug: None }));
        }
    }

    #[test]
    fn test_deserialize_from_empty_array() {
        let input = json!([]);
        let list: InterfaceList = serde_json::from_value(input).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_deserialize_from_array_with_duplicates() {
        let input = json!(["dup", "dup"]);
        let list: InterfaceList = serde_json::from_value(input).unwrap();
        // HashMap overwrites duplicate keys → only one remains
        assert_eq!(list.0.len(), 1);
        assert!(list.0.contains_key("dup"));
    }

    #[test]
    fn test_deserialize_array_with_non_string_element() {
        let input = json!(["valid", 42]);
        let result: Result<InterfaceList, _> = serde_json::from_value(input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("expected a string") || err.contains("invalid type"),
            "Unexpected error message: {}",
            err
        );
    }

    #[test]
    fn test_deserialize_invalid_object_value() {
        // Valid key, but value is not a valid ComponentItemSpec
        let input = json!({
            "bad:type": "not-an-enum"
        });
        let result: Result<InterfaceList, _> = serde_json::from_value(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_unknown_enum_variant() {
        let input = json!({
            "bad:type": { "nonexistent_variant": {} }
        });
        let result: Result<InterfaceList, _> = serde_json::from_value(input);
        assert!(result.is_err());
        // serde will report "unknown variant"
        assert!(result.unwrap_err().to_string().contains("unknown variant"));
    }

    #[test]
    fn test_serialize_known_variant() {
        let mut list = InterfaceList::new();
        list.insert(
            "test:func".to_string(),
            ComponentItemSpec::ComponentFunc {
                params: vec![("a".to_string(), "u32".to_string())],
                results: vec!["u32".to_string()],
            },
        );

        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(
            json,
            json!({
                "test:func": {
                    "func": {
                        "params": [["a", "u32"]],
                        "results": ["u32"]
                    }
                }
            })
        );

        let restored: InterfaceList = serde_json::from_value(json).unwrap();
        assert_eq!(list, restored);
    }

    #[test]
    fn test_roundtrip_with_mixed_variants() {
        let mut original = InterfaceList::new();
        original.insert(
            "func:handler".to_string(),
            ComponentItemSpec::ComponentFunc {
                params: vec![],
                results: vec![],
            },
        );
        original.insert(
            "legacy".to_string(),
            ComponentItemSpec::Unknown {
                debug: Some("legacy module".to_string()),
            },
        );

        let json = serde_json::to_string(&original).unwrap();
        let restored: InterfaceList = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn test_serialize_and_deserialize_empty_unknown() {
        let list = InterfaceList::from(vec!["simple".to_string()]);
        let json = serde_json::to_string(&list).unwrap();
        // Should produce: {"simple":{"unknown":{}}}
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["simple"]["unknown"], json!({}));

        let restored: InterfaceList = serde_json::from_str(&json).unwrap();
        assert_eq!(list, restored);
    }

    #[test]
    fn test_reject_scalar_input() {
        for invalid in [json!(42), json!("string"), json!(null), json!(true)] {
            let result: Result<InterfaceList, _> = serde_json::from_value(invalid);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            // Accept either the old-style or new-style error messages
            assert!(
                err.contains("must be an array")
                    || err.contains("must be an object")
                    || err.contains("expected either a map")
                    || err.contains("sequence of strings"),
                "unexpected error: {}",
                err
            );
        }
    }

    #[test]
    fn test_interface_list_from_hashmap() {
        let mut map = HashMap::new();
        map.insert(
            "test".to_string(),
            ComponentItemSpec::Unknown {
                debug: Some("direct".to_string()),
            },
        );
        let list = InterfaceList::from(map);
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0["test"], ComponentItemSpec::Unknown {
            debug: Some("direct".to_string())
        });
    }

    #[test]
    fn test_as_inner_and_into_inner() {
        let mut list = InterfaceList::new();
        list.insert("x".to_string(), ComponentItemSpec::Unknown { debug: None });
        assert_eq!(list.as_inner().len(), 1);
        let map = list.into_inner();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("x"));
    }
}
