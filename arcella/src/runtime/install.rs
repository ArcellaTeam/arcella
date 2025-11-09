// arcella/src/runtime/validation.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::{Path, PathBuf};
use tokio::fs;

use crate::{
    {ArcellaError, ArcellaResult},
    runtime::state::ArcellaState,
    storage::StorageManager,
};

/// Represents a validated module package ready for installation.
#[derive(Debug, Clone)]
pub struct InstallPackage {
    /// Base directory containing all package files (e.g., staging dir or original dir).
    pub package_dir: Option<PathBuf>,

    /// Path to the `.wasm` module inside `package_dir`.
    pub wasm_path: PathBuf,

    /// Path to the optional `module.component.toml` inside `package_dir`.
    pub component_toml_path: Option<PathBuf>,

    // В будущем:
    // pub wit_path: Option<PathBuf>,
    // pub license_path: Option<PathBuf>,
}

/// Validates that a given path points to a valid installable module package.
///
/// Rules:
/// - Path must be a file with `.wasm` extension.
/// - If the module is a WASI module (no WIT in .wasm), `component.toml` **must** exist.
/// - If the module is a Component Model module (has WIT), `component.toml` is **optional**.
///
/// This function **does not parse** the contents — only checks file existence and naming.
///
/// Returns `Ok(InstallPackage)` if valid, or `Err` with clear diagnostics.
pub async fn validate_install_package(wasm_path: &Path) -> ArcellaResult<InstallPackage> {
    // 1. Проверка: это файл?
    if !wasm_path.is_file() {
        return Err(ArcellaError::InvalidArgument{
            message: format!("Path is not a file: {:?}", wasm_path),
        });
    }

    // 2. Проверка расширения
    if wasm_path.extension().map_or(true, |ext| ext != "wasm") {
        return Err(ArcellaError::InvalidArgument{
            message: format!("File must have .wasm extension: {:?}", wasm_path),
        });
    }

    // 3. Поиск '.component.toml' рядом
    let stem = wasm_path.file_stem()
        .ok_or_else(|| ArcellaError::InvalidArgument{
            message: "Invalid filename".into(),
        })?;
    let expected_toml = wasm_path.with_file_name(format!("{}.component.toml", stem.to_string_lossy()));

    let component_toml_path = if expected_toml.exists() {
        Some(expected_toml)
    } else {
        None
    };

    // 4. (Опционально) можно уже здесь попытаться определить тип компонента
    //    — но для MVP достаточно передать дальше

    Ok(InstallPackage {
        package_dir: None,
        wasm_path: wasm_path.to_path_buf(),
        component_toml_path,
    })
}

/// Takes a validated package and stages it into a clean temporary subdirectory.
///
/// The subdirectory name is derived from the expected module ID (e.g., `name@version`).
/// If the directory already exists, it is removed (with a warning) before staging.
///
/// Returns a new `InstallPackage` with all paths pointing inside the temp staging area.
pub async fn prepare_install_package_in_temp(
    storage: &StorageManager,
    package: InstallPackage,
) -> ArcellaResult<InstallPackage> {
    let staging_dir = tempfile::tempdir_in(storage.temp_path())
        .map_err(|e| ArcellaError::IoWithPath {
            source: e,
            path: storage.temp_path().to_path_buf(),
        })?
        .keep(); // Передаём владение в PathBuf

    // Копирование .wasm
    let wasm_file_name = package.wasm_path
        .file_name()
        .ok_or_else(|| ArcellaError::InvalidArgument { 
            message: "WASM file has no name".into()
        })?;
    let staged_wasm = staging_dir.join(wasm_file_name);
    tokio::fs::copy(&package.wasm_path, &staged_wasm).await
        .map_err(|e| ArcellaError::IoWithPath {
            source: e,
            path: package.wasm_path.clone(),
        })?;

    // Копирование .component.toml (если есть)
    let staged_toml = if let Some(ref src_toml) = package.component_toml_path {
        let toml_file_name = src_toml.file_name()
            .ok_or_else(|| ArcellaError::InvalidArgument {
                message: "TOML file has no name".into()
            })?;
        let dst_toml = staging_dir.join(toml_file_name);
        tokio::fs::copy(src_toml, &dst_toml).await
            .map_err(|e| ArcellaError::IoWithPath {
                source: e,
                path: src_toml.clone(),
            })?;
        Some(dst_toml)
    } else {
        None
    };

    // Возвращаем новый пакет с путями внутри staging-директории
    Ok(InstallPackage {
        package_dir: Some(staging_dir.clone()), // Сохраняем путь к новой директории
        wasm_path: staged_wasm,
        component_toml_path: staged_toml,
    })
}

/// Sanitizes a module ID for use as a filesystem directory name.
///
/// Replaces characters that are invalid or problematic in filenames:
/// - '@' → '__at__'
/// - '/' → '__slash__'
/// - '\' → '__bslash__'
/// - ':' → '__colon__'
/// - и т.д.
///
/// WARNING: This is not cryptographically safe — only for local filesystem use.
fn sanitize_module_id(id: &str) -> String {
    id.replace('@', "__at__")
        .replace('/', "__slash__")
        .replace('\\', "__bslash__")
        .replace(':', "__colon__")
        .replace('?', "__qmark__")
        .replace('*', "__star__")
        .replace('|', "__pipe__")
        .replace('<', "__lt__")
        .replace('>', "__gt__")
        .replace('"', "__quote__")
}

/// Ensures the final module directory does not exist in state or on disk.
pub async fn check_module_not_installed(
    state: &ArcellaState,
    modules_dir: &Path,
    module_id: &str,
) -> ArcellaResult<()> {
    if state.installed_modules.contains_key(module_id) {
        let e = ArcellaError::ModuleAlreadyInstalled(module_id.to_string());
        tracing::warn!("{}", e);
        return Err(e);
    }

    let dest_dir = modules_dir.join(module_id);
    if dest_dir.exists() {
        let e = ArcellaError::ModuleDirAlreadyExists(module_id.to_string());
        tracing::warn!("{}", e);
        return Err(e);
    }

    Ok(())
}

/// Copies staged package files to the final module directory with durability guarantees.
pub async fn install_module_files_to_storage(
    staged: &InstallPackage,
    modules_dir: &Path,
    module_id: &str,
) -> ArcellaResult<PathBuf> {
    let dest_dir = modules_dir.join(module_id);
    fs::create_dir_all(&dest_dir).await?;

    // Copy .wasm
    let wasm_file_name = staged.wasm_path
        .file_name()
        .ok_or_else(|| ArcellaError::InvalidArgument { 
            message: "WASM file has no name".into()
        })?;
    let dest_wasm = dest_dir.join(wasm_file_name);
    fs::copy(&staged.wasm_path, &dest_wasm).await?;
    fs::File::open(&dest_wasm).await?.sync_all().await?;

    // Copy .component.toml if present
    if let Some(ref src_toml) = staged.component_toml_path {
        let toml_file_name = src_toml.file_name()
            .ok_or_else(|| ArcellaError::InvalidArgument {
                message: "TOML file has no name".into()
            })?;
        let dest_toml = dest_dir.join(toml_file_name);
        fs::copy(src_toml, &dest_toml).await?;
        fs::File::open(&dest_toml).await?.sync_all().await?;
    }

    // Sync directory metadata (critical for durability of file names)
    #[cfg(unix)]
    std::fs::File::open(&dest_dir)?.sync_all()?;

    Ok(dest_dir)
}


