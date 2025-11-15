// arcella/src/utils/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::{Path, PathBuf};
use crate::ArcellaError;
use crate::ArcellaResult;

/// Maximum allowed length for base names (e.g., module name, deployment ID).
/// Chosen to prevent filesystem path overflow and ensure readability.
const MAX_BASE_NAME_LENGTH: usize = 128;

/// Extracts the base name from a file path by stripping a **required** extension.
///
/// Examples:
/// - `"/x/app.wasm"` → `"app"` (if ext = `"wasm"`)
/// - `"web.deployment.toml"` → `"web"` (if ext = `"deployment.toml"`)
///
/// # Errors
/// - If file has no name
/// - If extension does not match exactly
pub fn base_name_from_file_with_ext<P: AsRef<Path>>(
    path: P,
    ext: &str,
) -> ArcellaResult<String> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ArcellaError::InvalidArgument {
            message: "File has no valid UTF-8 name".into(),
        })?;

    let expected_suffix = format!(".{}", ext);
    if !file_name.ends_with(&expected_suffix) {
        return Err(ArcellaError::InvalidArgument {
            message: format!(
                "File '{}' does not have extension '{}'",
                file_name, ext
            ),
        });
    }
    if file_name.len() <= expected_suffix.len() {
        return Err(ArcellaError::InvalidArgument {
            message: format!(
                "Filename '{}' is too short to have extension '{}'",
                file_name, ext
            ),
        });
    }

    let base = &file_name[..file_name.len() - expected_suffix.len()];
    if base.is_empty() {
        return Err(ArcellaError::InvalidArgument {
            message: "Base name before extension is empty".into(),
        });
    }

    Ok(base.to_string())
}

/// Validates that a base name (e.g., module name, deployment ID) contains only safe characters.
///
/// Allowed: ASCII letters, digits, hyphens (`-`), underscores (`_`).
/// This matches the `ModuleId::is_valid_name` rule.
pub fn validate_base_name(name: &str) -> ArcellaResult<()> {
    if name.is_empty() || name.len() > MAX_BASE_NAME_LENGTH {
        return Err(ArcellaError::InvalidArgument {
            message: "Base name is empty or too long".into(),
        });
    }

    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(ArcellaError::InvalidArgument {
            message: "Base name contains invalid characters (allowed: a-z, A-Z, 0-9, -, _)".into(),
        });
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(ArcellaError::InvalidArgument {
            message: "Base name cannot start or end with hyphen".into(),
        });
    }

    Ok(())
}

/// Constructs a sibling file path by appending a suffix to the file stem.
///
/// Examples:
/// - `"app.wasm"` + `".component.toml"` → `"app.component.toml"`
/// - `".env.wasm"` + `".toml"` → `".env.toml"`
///
/// If the input has no stem (e.g., `/` or `.`), the result is undefined
/// and should not be relied upon. In practice, callers ensure valid paths.
pub fn sibling_path_with_suffix<P: AsRef<Path>>(original: P, suffix: &str) -> PathBuf {
    let original = original.as_ref();
    let stem = original.file_stem().unwrap_or(original.file_name().unwrap_or_default());
    original.with_file_name(format!("{}{}", stem.to_string_lossy(), suffix))
}

/// Constructs the expected suffix path for a given base name.
///
/// Example: `"web"` → `"web.deployment.toml"`
pub fn file_path_from_base_and_extension<P: AsRef<Path>>(base_dir: P, base_name: &str, suffix: &str) -> PathBuf {
    base_dir.as_ref().join(format!("{}.{}", base_name, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_name_from_file_with_ext() {
        assert_eq!(
            base_name_from_file_with_ext("app.wasm", "wasm").unwrap(),
            "app"
        );
        assert_eq!(
            base_name_from_file_with_ext("hello-world@1.0.0.deployment.toml", "deployment.toml").unwrap(),
            "hello-world@1.0.0"
        );
        assert!(base_name_from_file_with_ext("bad.txt", "wasm").is_err());
        assert!(base_name_from_file_with_ext(".wasm", "wasm").is_err());
    }

    #[test]
    fn test_validate_base_name() {
        assert!(validate_base_name("valid_mod").is_ok());
        assert!(validate_base_name("test-123").is_ok());
        assert!(validate_base_name("").is_err());
        assert!(validate_base_name("invalid name!").is_err());
        assert!(validate_base_name(&"x".repeat(129)).is_err());
    }

    #[test]
    fn test_sibling_path_with_suffix() {
        let path = Path::new("/a/b/app.wasm");
        let toml = sibling_path_with_suffix(path, ".component.toml");
        assert_eq!(toml, Path::new("/a/b/app.component.toml"));
    }

}
