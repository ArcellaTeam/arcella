// arcella/arcella-core/src/utils/fs/mod.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! File system and TOML configuration utilities for Arcella.
//!
//! This crate provides common functions for:
//! - Resolving base directories based on executable location or environment.
//! - Finding and validating `.toml` configuration files.
//! - Collecting files specified by `includes` patterns.
//! - Converting TOML values into a serializable format used by Arcella.
//! - Loading configurations recursively with warnings.
//!
//! It is designed to be used by the Arcella runtime and other tools that need
//! to process TOML-based configurations in a consistent way.

use std::env;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

use super::error::{ArcellaUtilsError, ArcellaUtilsResult};

/// Determines the base directory for Arcella based on the executable location or environment.
///
/// The function follows this priority order:
/// 1. If the executable is located in a `bin` subdirectory and if parent of `bin` is not root
///    directory, the parent of `bin` is use. 
/// 2. If the current directory (where the executable is run from) contains a `config` subdirectory,
///    the current directory is used.
/// 3. Otherwise, the user's home directory joined with `.arcella` is used.
///
/// # Returns
///
/// A `Result` containing the determined `PathBuf` or an error if the home directory
/// cannot be determined.
pub async fn find_base_dir() -> ArcellaUtilsResult<PathBuf> {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            // Case 1: executable is in a `bin` directory
            if parent.file_name() == Some(std::ffi::OsStr::new("bin")) {
                if let Some(grandparent) = parent.parent() {
                    // Avoid using root directory (e.g., /bin → /) as base dir
                    if grandparent.parent().is_some() {
                        return Ok(grandparent.to_path_buf());
                    }
                }
            }

            // Case 2: check if current_exe's parent has a `config` dir
            let local_config = parent.join("config");
            if let Ok(metadata) = fs::metadata(&local_config).await {
                if metadata.is_dir() {
                    return Ok(parent.to_path_buf());
                }
            }
        }
    }

    // Case 3: fallback to ~/.arcella
    dirs::home_dir()
        .map(|d| d.join(".arcella"))
        .ok_or_else(|| ArcellaUtilsError::Internal("Cannot determine home directory".into()))
}


/// Creates a crash-safe temporary subdirectory with a unique name.
///
/// The directory name follows the pattern:
/// - `{prefix}.tmp-{uuid}` if `prefix` is provided,
/// - `tmp-{uuid}` if no prefix.
///
/// The UUID (v4) ensures global uniqueness across processes and reboots.
/// Invalid characters in `prefix` are replaced with underscores to ensure portability.
///
/// # Safety
///
/// The created directory is empty and owned by the current user.
/// Caller is responsible for cleanup (e.g., via `tempfile::TempDir` semantics or manual `remove_dir_all`).
///
/// # Errors
///
/// Fails if:
/// - `parent_dir` does not exist or is not writable,
/// - OS fails to create the directory (e.g., too long path, permission denied).
pub async fn create_temp_subdir(
    parent_dir: &Path,
    prefix: Option<&str>,
) -> ArcellaUtilsResult<PathBuf> {
    let uuid_part = Uuid::new_v4();

    let temp_name = if let Some(p) = prefix {
        // Sanitize prefix to filesystem-safe characters
        let clean_prefix = p
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        format!("{}.tmp-{}", clean_prefix, uuid_part.simple())
    } else {
        format!("tmp-{}", uuid_part.simple())
    };

    let temp_path = parent_dir.join(temp_name);

    fs::create_dir_all(&temp_path).await
        .map_err(|e| ArcellaUtilsError::IoWithPath {
            source: e,
            path: temp_path.clone(),
        })?;

    Ok(temp_path)
}

/// Copies multiple files to a target directory in parallel.
///
/// Preserves original filenames. Optionally ensures **durability** via `fsync`.
///
/// ## When to use `sync = true`
/// - **Final installation** into `modules/` — to guarantee module integrity after crash.
/// - **Critical config updates** — where partial write must never be visible.
///
/// ## When to use `sync = false`
/// - **Staging/temporary copies** (e.g., before validation or rename).
/// - **Performance-sensitive bulk operations** where crash recovery is acceptable.
///
/// # Contract
/// - **Precondition**: `target_dir` must exist and be a writable directory.
/// - **Input**: All `files` must exist and be readable.
/// - **Output**: Returns paths to copied files (same order as input).
/// - **Failure**: Partial copies may remain; caller is responsible for cleanup.
///
/// # Errors
/// - `InvalidArgument` if any source file has no name or `target_dir` is not a directory.
/// - `IoWithPath` on I/O errors (not found, permission denied, disk full, etc.).
pub async fn copy_files_to_dir(
    files: &[PathBuf],
    target_dir: &Path,
    sync: bool,
) -> ArcellaUtilsResult<Vec<PathBuf>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }

    // Validate target directory upfront
    if !target_dir.is_dir() {
        return Err(ArcellaUtilsError::InvalidArgument {
            message: format!("Target is not a directory: {:?}", target_dir),
        });
    }

    // Parallel copy phase
    let copy_futures: Vec<_> = files
        .iter()
        .map(|src_path| {
            let target_dir = target_dir.to_path_buf();
            async move {
                let file_name = src_path
                    .file_name()
                    .ok_or_else(|| ArcellaUtilsError::InvalidArgument {
                        message: format!("File has no name: {:?}", src_path),
                    })?;

                let dest_path = target_dir.join(file_name);

                fs::copy(src_path, &dest_path)
                    .await
                    .map_err(|e| ArcellaUtilsError::IoWithPath {
                        source: e,
                        path: src_path.clone(),
                    })?;

                let result: ArcellaUtilsResult<PathBuf> = Ok(dest_path);
                result
            }
        })
        .collect();

    let staged_paths = futures::future::try_join_all(copy_futures).await?;

    // Optional: ensure all file data is on persistent storage
    if sync {
        let file_sync_futures: Vec<_> = staged_paths
            .iter()
            .map(|path| async {
                let path = path.clone();
                let file = tokio::fs::File::open(path).await?;
                file.sync_all().await?;
                Ok::<(), ArcellaUtilsError>(())
            })
            .collect();    

        futures::future::try_join_all(file_sync_futures).await?;
    }

    Ok(staged_paths)
}

/// Synchronizes a directory’s metadata to persistent storage.
///
/// On Unix-like systems, this calls `fsync()` on the directory file descriptor,
/// which ensures that **directory entries** (i.e., filenames and their inode links)
/// are committed to disk. This is critical after creating or renaming files.
///
/// On Windows, this is a no-op (directory fsync is not supported in standard APIs),
/// but the function remains for cross-platform compatibility.
///
/// # Why this matters
///
/// After writing files into a directory, a crash could leave the directory
/// in a state where files exist on disk but are not linked by name.
/// `fsync` on the directory prevents this.
///
/// # Errors
///
/// - `IoWithPath`: if the directory cannot be opened or synced.
pub async fn sync_directory(dir: &Path) -> ArcellaUtilsResult<()> {
    let file = fs::File::open(dir).await
        .map_err(|e| ArcellaUtilsError::IoWithPath {
            source: e,
            path: dir.to_path_buf(),
        })?;
    file.sync_all().await
        .map_err(|e| ArcellaUtilsError::IoWithPath {
            source: e,
            path: dir.to_path_buf(),
        })?;
    Ok(())
}

/// Atomically renames `src` to `dst` using `std::fs::rename`.
///
/// This operation is:
/// - **Atomic** on POSIX-compliant filesystems (ext4, XFS, APFS, etc.).
/// - **Safe** to use after all file data and directory entries in `src` are synchronized.
///
/// # Errors
///
/// Returns `ArcellaUtilsError::IoWithPath` if the rename fails (e.g., cross-device,
/// permission denied, `dst` already exists on Windows, etc.).
pub async fn atomic_rename(src: PathBuf, dst: PathBuf) -> Result<(), ArcellaUtilsError> {
    let src_for_error = src.clone();

    tokio::task::spawn_blocking(move || {
        std::fs::rename(&src, &dst)
    }).await
        .map_err(|e| ArcellaUtilsError::IoWithPath {
            source: std::io::Error::other(format!("Spawn blocking for rename failed: {}", e)),
            path: src_for_error.clone(),
        })?
        .map_err(|e| ArcellaUtilsError::IoWithPath {
            source: e,
            path: src_for_error.clone(),
        })
}


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
) -> ArcellaUtilsResult<String> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ArcellaUtilsError::InvalidArgument {
            message: "File has no valid UTF-8 name".into(),
        })?;

    let expected_suffix = format!(".{}", ext);
    if !file_name.ends_with(&expected_suffix) {
        return Err(ArcellaUtilsError::InvalidArgument {
            message: format!(
                "File '{}' does not have extension '{}'",
                file_name, ext
            ),
        });
    }
    if file_name.len() <= expected_suffix.len() {
        return Err(ArcellaUtilsError::InvalidArgument {
            message: format!(
                "Filename '{}' is too short to have extension '{}'",
                file_name, ext
            ),
        });
    }

    let base = &file_name[..file_name.len() - expected_suffix.len()];
    if base.is_empty() {
        return Err(ArcellaUtilsError::InvalidArgument {
            message: "Base name before extension is empty".into(),
        });
    }

    Ok(base.to_string())
}

/// Validates that a base name (e.g., module name, deployment ID) contains only safe characters.
///
/// Allowed: ASCII letters, digits, hyphens (`-`), underscores (`_`).
/// This matches the `ModuleId::is_valid_name` rule.
pub fn validate_base_name(name: &str) -> ArcellaUtilsResult<()> {
    if name.is_empty() || name.len() > MAX_BASE_NAME_LENGTH {
        return Err(ArcellaUtilsError::InvalidArgument {
            message: "Base name is empty or too long".into(),
        });
    }

    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(ArcellaUtilsError::InvalidArgument {
            message: "Base name contains invalid characters (allowed: a-z, A-Z, 0-9, -, _)".into(),
        });
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(ArcellaUtilsError::InvalidArgument {
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

