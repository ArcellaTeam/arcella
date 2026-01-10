// arcella/arcella-core/src/runtime/install.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Module installation pipeline.
//!
//! This module implements the full **crash-safe installation workflow** for WebAssembly components.
//! It follows a staging � validation � atomic commit pattern to ensure filesystem consistency
//! even in the event of process crashes or power loss.
//!
//! The pipeline consists of the following phases:
//! 1. **Validation**: Confirm input is a `.wasm` file and locate associated manifests.
//! 2. **Staging**: Copy all package files to a temporary directory under `~/.arcella/tmp/`.
//! 3. **Parsing**: Extract module identity (`name@version`) from the component manifest or WASM.
//! 4. **Uniqueness Check**: Ensure the module is not already installed (state + disk).
//! 5. **Atomic Commit**: Copy staged files to a temporary module dir, fsync, then rename into place.
//! 6. **State Record**: Append installation to the write-ahead log (WAL).
//! 7. **Cleanup**: Remove staging directory.
//!
//! If any step fails, temporary artifacts are cleaned up to avoid disk leakage.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::utils::fs::{
    atomic_rename,
    base_name_from_file_with_ext,
    copy_files_to_dir,
    create_temp_subdir,
    sibling_path_with_suffix,
    sync_directory,
};

use arcella_types::module_id::ModuleId;

use crate::{
    ArcellaError,
	ArcellaResult,
    runtime::state::ArcellaState,
    storage::StorageManager,
};

/// Represents a validated module package ready for installation.
///
/// This struct encapsulates all files that belong to a single logical module.
/// It is produced during validation and propagated through staging and final installation.
///
/// The package may contain:
/// - **Required**: One `.wasm` file (Component Model or WASI).
/// - **Optional**: A `.component.toml` manifest (required for WASI modules).
/// - **Optional**: A `.deployment.template.toml` with default runtime recommendations.
///
/// All paths are resolved at construction time and assumed to be valid for the duration
/// of the installation process.
#[derive(Debug, Clone)]
pub struct InstallPackage {
    /// Original source directory (for diagnostics only; may be `None`).
    pub package_dir: Option<PathBuf>,

    /// Path to the `.wasm` module always present after validation.
    pub wasm_path: PathBuf,

    /// Optional path to the component manifest (`{stem}.component.toml`).
    /// Required for WASI modules; optional for Component Model modules.
    pub component_toml_path: Option<PathBuf>,

    /// Optional path to the deployment template (`{stem}.deployment.template.toml`).
    /// Provides default isolation, trust, and async settings.
    pub deployment_template_path: Option<PathBuf>,
}

impl InstallPackage {
    /// Returns all existing file paths as owned `PathBuf`s.
    ///
    /// Only includes paths that are `Some`. The `.wasm` path is always included.
    /// Order is deterministic but should not be relied upon externally.
    pub fn existing_file_paths_owned(&self) -> Vec<PathBuf> {
        let mut paths = Vec::with_capacity(3);
        paths.push(self.wasm_path.clone());

        if let Some(ref p) = self.component_toml_path {
            paths.push(p.clone());
        }
        if let Some(ref p) = self.deployment_template_path {
            paths.push(p.clone());
        }

        paths
    }

    /// Builds a map from file name (e.g., `"logger.wasm"`) to its full path.
    ///
    /// This is used during staging to reconstruct the package in a new directory
    /// while preserving original file names. Critical for `from_staging_dir`.
    pub fn file_name_map(&self) -> HashMap<&std::ffi::OsStr, &Path> {
        let mut map = HashMap::with_capacity(3);
        map.insert(self.wasm_path.file_name().unwrap(), self.wasm_path.as_path());

        if let Some(ref p) = self.component_toml_path {
            map.insert(p.file_name().unwrap(), p.as_path());
        }
        if let Some(ref p) = self.deployment_template_path {
            map.insert(p.file_name().unwrap(), p.as_path());
        }

        map
    }

    /// Reconstructs an `InstallPackage` from a staging directory.
    ///
    /// Assumes all files from `file_name_map()` have been copied into `staging_dir`
    /// with identical base names. Used after `copy_files_to_dir` to create a new
    /// package pointing to staged files.
    ///
    /// # Errors
    ///
    /// Returns `ArcellaError::InvalidArgument` if any expected file is missing
    /// in the staging directory.
    pub fn from_staging_dir(&self, staging_dir: PathBuf) -> ArcellaResult<Self> {
        let staged_wasm = staging_dir.join(
            self.wasm_path.file_name()
                .ok_or_else(|| {
                    let e = ArcellaError::InvalidArgument {
                        message: "WASM file has no name".into(),
                    };
                    tracing::error!("{}", e);
                    e
                })?,
        );

        let staged_component_toml = self
            .component_toml_path
            .as_ref()
            .map(|orig| staging_dir.join(orig.file_name().unwrap()));

        let staged_deployment_template = self
            .deployment_template_path
            .as_ref()
            .map(|orig| staging_dir.join(orig.file_name().unwrap()));

        // Verify all expected files exist in staging
        if !staged_wasm.exists() {
            let e = ArcellaError::InvalidArgument {
                message: format!("Staged WASM file not found: {:?}", staged_wasm),
            };
            tracing::error!("{}", e);
            return Err(e);
        }
        if let Some(ref p) = staged_component_toml {
            if !p.exists() {
                let e = ArcellaError::InvalidArgument {
                    message: format!("Staged component TOML not found: {:?}", p),
                };
                tracing::error!("{}", e);
                return Err(e);
            }
        }
        if let Some(ref p) = staged_deployment_template {
            if !p.exists() {
                let e = ArcellaError::InvalidArgument {
                    message: format!("Staged deployment template not found: {:?}", p),
                };
                tracing::error!("{}", e);
                return Err(e);
            }
        }

        Ok(InstallPackage {
            package_dir: Some(staging_dir),
            wasm_path: staged_wasm,
            component_toml_path: staged_component_toml,
            deployment_template_path: staged_deployment_template,
        })
    }

    /// Validates that all declared paths point to existing files.
    ///
    /// Should be called immediately after constructing an `InstallPackage`
    /// from user-provided paths to fail fast on missing inputs.
    pub fn validate_paths_exist(&self) -> ArcellaResult<()> {
        if !self.wasm_path.exists() {
            let e = ArcellaError::InvalidArgument {
                message: format!("WASM file not found: {:?}", self.wasm_path),
            };
            tracing::error!("{}", e);
            return Err(e);
        }
        if let Some(ref p) = self.component_toml_path {
            if !p.exists() {
                let e = ArcellaError::InvalidArgument {
                    message: format!("Component TOML not found: {:?}", p),
                };
                tracing::error!("{}", e);
                return Err(e);
            }
        }
        if let Some(ref p) = self.deployment_template_path {
            if !p.exists() {
                let e = ArcellaError::InvalidArgument {
                    message: format!("Deployment template not found: {:?}", p),
                };
                tracing::error!("{}", e);
                return Err(e);
            }
        }
        Ok(())
    }
}

/// Validates a user-provided `.wasm` path as an installable module package.
///
/// This function checks:
/// - The path points to a regular file with a `.wasm` extension.
/// - Associated manifest files (if any) exist with the expected naming convention:
///   - `{stem}.component.toml`
///   - `{stem}.deployment.template.toml`
///
/// **Note**: This function does **not** parse the contents of any file.
/// It only validates file existence and naming.
///
/// # Returns
///
/// `Ok(InstallPackage)` if the path is valid, or an error with a human-readable message.
pub async fn validate_install_package(wasm_path: &Path) -> ArcellaResult<InstallPackage> {
    if !wasm_path.is_file() {
        let e = ArcellaError::InvalidArgument{
            message: format!("Path is not a file: {:?}", wasm_path),
        };
        tracing::error!("{}", e);
        return Err(e);
    }

    // Validate extension and extract base name
    match base_name_from_file_with_ext(wasm_path, "wasm") {
        Ok(_) => (),
        Err(e) => {
            tracing::error!("{}", e);
            return Err(ArcellaError::UtilsError(e));
        }
    }

    // Construct expected sibling paths
    let component_toml_path = sibling_path_with_suffix(wasm_path, ".component.toml");
    let deployment_template_path = sibling_path_with_suffix(wasm_path, ".deployment.template.toml");    

    Ok(InstallPackage {
        package_dir: None,
        wasm_path: wasm_path.to_path_buf(),
        component_toml_path: component_toml_path.exists().then_some(component_toml_path),
        deployment_template_path: deployment_template_path.exists().then_some(deployment_template_path),
    })
}

/// Stages a validated package into a clean, isolated temporary directory.
///
/// The staging directory is created under `StorageManager::temp_path()` with a unique UUID-based name.
/// All package files are copied into this directory with their original names preserved.
///
/// This step isolates the installation from the source location and prepares for atomic commit.
///
/// # Returns
///
/// A new `InstallPackage` pointing to files inside the staging directory.
pub async fn prepare_install_package_in_temp(
    storage: &StorageManager,
    package: InstallPackage,
) -> ArcellaResult<InstallPackage> {
    package.validate_paths_exist()?;

    let staging_dir = match create_temp_subdir(storage.temp_path(), Some("install-staging")).await {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());
        }
    };

    let source_files = package.existing_file_paths_owned();
    let _ = match copy_files_to_dir(&source_files, &staging_dir, false).await {
        Ok(vec) => vec,
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());    
        }
    };

    package.from_staging_dir(staging_dir)
}

/// Ensures a module with the given ID is not already installed.
///
/// Checks both:
/// 1. In-memory runtime state (`ArcellaState::installed_modules`)
/// 2. On-disk module directory (`~/.arcella/modules/{module_id}`)
///
/// Fails if either check finds an existing installation.
///
/// # Errors
///
/// - `ArcellaError::ModuleAlreadyInstalled` if present in state.
/// - `ArcellaError::ModuleDirAlreadyExists` if present on disk.
pub async fn check_module_not_installed(
    state: &ArcellaState,
    modules_dir: &Path,
    module_id: &ModuleId,
) -> ArcellaResult<()> {
    let mod_id = module_id.to_string();
    if state.installed_modules.contains_key(&mod_id) {
        let e = ArcellaError::ModuleAlreadyInstalled(mod_id);
        tracing::warn!("{}", e);
        return Err(e);
    }

    let dest_dir = modules_dir.join(&mod_id);
    if dest_dir.exists() {
        let e = ArcellaError::ModuleDirAlreadyExists(mod_id);
        tracing::warn!("{}", e);
        return Err(e);
    }

    Ok(())
}

/// Atomically installs staged module files into permanent storage.
///
/// Follows a crash-safe sequence:
/// 1. Create a temporary directory inside `modules_dir` (e.g., `modules/name@ver.tmp-uuid`).
/// 2. Copy all staged files into this temp dir.
/// 3. `fsync()` the temp dir to ensure filenames are persisted.
/// 4. Atomically rename the temp dir to `modules/{module_id}`.
/// 5. `fsync()` the parent `modules_dir` to ensure the new entry is visible.
///
/// After this function returns successfully, the module is durably installed on disk.
///
/// # Returns
///
/// The path to the final installed module directory.
pub async fn install_module_files_to_storage(
    staged: &InstallPackage,
    modules_dir: &Path,
    module_id: &ModuleId,
) -> ArcellaResult<PathBuf> {
    staged.validate_paths_exist()?;

    // Step 1: Create temporary destination under modules_dir
    let temp_dest_dir = match create_temp_subdir(modules_dir, Some(&module_id.to_string())).await {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());
        }
    };

    // Step 2: Copy all files into the temporary destination
    let source_files = staged.existing_file_paths_owned();
    let _ = match copy_files_to_dir(&source_files, &temp_dest_dir, true).await {
        Ok(vec) => vec,
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());
        }
    };

    // Step 3: Ensure directory entries are persisted
    match sync_directory(&temp_dest_dir).await {
        Ok(_) => (),
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());
        }
    };

    // Step 4: Atomically publish the module
    let final_dest_dir = modules_dir.join(&module_id.to_string());
    match atomic_rename(temp_dest_dir, final_dest_dir.clone()).await {
        Ok(_) => (),
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());
        }
    };

    // Step 5: Ensure the new module_id entry is visible in parent dir
    match sync_directory(modules_dir).await {
        Ok(_) => (),
        Err(e) => {
            tracing::error!("{}", e);
            return Err(e.into());
        }
    };

    Ok(final_dest_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    mod install_package_tests {
        use super::*;

        fn dummy_wasm_path(dir: &TempDir) -> PathBuf {
            let path = dir.path().join("test.wasm");
            let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Minimal valid WASM
            fs::write(&path, wasm_bytes).unwrap();
            path
        }

        fn dummy_toml_path(dir: &TempDir, name: &str) -> PathBuf {
            let path = dir.path().join(name);
            fs::write(&path, "# dummy TOML content").unwrap();
            path
        }

        #[test]
        fn test_existing_file_paths_owned() {
            let temp = TempDir::new().unwrap();
            let wasm = dummy_wasm_path(&temp);
            let toml = dummy_toml_path(&temp, "comp.component.toml");
            let tmpl = dummy_toml_path(&temp, "deploy.deployment.template.toml");

            let package = InstallPackage {
                package_dir: None,
                wasm_path: wasm.clone(),
                component_toml_path: Some(toml.clone()),
                deployment_template_path: Some(tmpl.clone()),
            };

            let paths = package.existing_file_paths_owned();
            assert_eq!(paths.len(), 3);
            assert!(paths.contains(&wasm));
            assert!(paths.contains(&toml));
            assert!(paths.contains(&tmpl));
        }

        #[test]
        fn test_existing_file_paths_owned_only_wasm() {
            let temp = TempDir::new().unwrap();
            let wasm = dummy_wasm_path(&temp);

            let package = InstallPackage {
                package_dir: None,
                wasm_path: wasm.clone(),
                component_toml_path: None,
                deployment_template_path: None,
            };

            let paths = package.existing_file_paths_owned();
            assert_eq!(paths, vec![wasm]);
        }

        #[test]
        fn test_file_name_map() {
            let temp = TempDir::new().unwrap();
            let wasm = dummy_wasm_path(&temp);
            let toml = dummy_toml_path(&temp, "my.component.toml");

            let package = InstallPackage {
                package_dir: None,
                wasm_path: wasm.clone(),
                component_toml_path: Some(toml.clone()),
                deployment_template_path: None,
            };

            let map = package.file_name_map();
            assert_eq!(map.len(), 2);
            assert_eq!(map.get(std::ffi::OsStr::new("test.wasm")), Some(&wasm.as_path()));
            assert_eq!(
                map.get(std::ffi::OsStr::new("my.component.toml")),
                Some(&toml.as_path())
            );
        }

        #[test]
        fn test_from_staging_dir() {
            let source_temp = TempDir::new().unwrap();
            let staging_temp = TempDir::new().unwrap();

            let wasm = dummy_wasm_path(&source_temp);
            let toml = dummy_toml_path(&source_temp, "app.component.toml");

            let staged_wasm = staging_temp.path().join("test.wasm");
            let staged_toml = staging_temp.path().join("app.component.toml");
            fs::copy(&wasm, &staged_wasm).unwrap();
            fs::copy(&toml, &staged_toml).unwrap();

            let original_package = InstallPackage {
                package_dir: Some(source_temp.path().to_path_buf()),
                wasm_path: wasm,
                component_toml_path: Some(toml),
                deployment_template_path: None,
            };

            let staged_package = original_package
                .from_staging_dir(staging_temp.path().to_path_buf())
                .unwrap();

            assert_eq!(staged_package.wasm_path, staged_wasm);
            assert_eq!(staged_package.component_toml_path, Some(staged_toml));
            assert_eq!(staged_package.package_dir, Some(staging_temp.path().to_path_buf()));
        }

        #[test]
        fn test_from_staging_dir_missing_wasm() {
            let temp = TempDir::new().unwrap();
            let wasm = dummy_wasm_path(&temp);
            let toml = dummy_toml_path(&temp, "x.component.toml");

            let package = InstallPackage {
                package_dir: None,
                wasm_path: wasm,
                component_toml_path: Some(toml),
                deployment_template_path: None,
            };

            let staging_dir = TempDir::new().unwrap().into_path();
            let err = package.from_staging_dir(staging_dir).unwrap_err();
            assert!(err.to_string().contains("Staged WASM file not found"));
        }

        #[test]
        fn test_validate_paths_exist_all_present() {
            let temp = TempDir::new().unwrap();
            let wasm = dummy_wasm_path(&temp);
            let toml = dummy_toml_path(&temp, "valid.component.toml");

            let package = InstallPackage {
                package_dir: None,
                wasm_path: wasm,
                component_toml_path: Some(toml),
                deployment_template_path: None,
            };

            assert!(package.validate_paths_exist().is_ok());
        }

        #[test]
        fn test_validate_paths_exist_missing_wasm() {
            let package = InstallPackage {
                package_dir: None,
                wasm_path: PathBuf::from("/definitely/missing.wasm"),
                component_toml_path: None,
                deployment_template_path: None,
            };

            let err = package.validate_paths_exist().unwrap_err();
            assert!(err.to_string().contains("WASM file not found"));
        }

        #[test]
        fn test_validate_paths_exist_missing_optional_toml() {
            let temp = TempDir::new().unwrap();
            let wasm = dummy_wasm_path(&temp);

            let package = InstallPackage {
                package_dir: None,
                wasm_path: wasm,
                component_toml_path: Some(PathBuf::from("/nonexistent.toml")),
                deployment_template_path: None,
            };

            let err = package.validate_paths_exist().unwrap_err();
            assert!(err.to_string().contains("Component TOML not found"));
        }
    }

    mod validate_install_package_tests {
        use super::*;

        #[tokio::test]
        async fn test_validate_install_package_finds_files_by_stem() {
            let temp = TempDir::new().unwrap();
            let wasm_path = temp.path().join("hello@1.0.0.wasm");
            let toml_path = temp.path().join("hello@1.0.0.component.toml");
            let tmpl_path = temp.path().join("hello@1.0.0.deployment.template.toml");

            fs::write(&wasm_path, vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]).unwrap();
            fs::write(&toml_path, "[component]\nname=\"hello\"\nversion=\"1.0.0\"").unwrap();
            fs::write(&tmpl_path, "[deployment]\nisolation=\"worker\"").unwrap();

            let package = validate_install_package(&wasm_path).await.unwrap();
            assert_eq!(package.wasm_path, wasm_path);
            assert_eq!(package.component_toml_path, Some(toml_path));
            assert_eq!(package.deployment_template_path, Some(tmpl_path));
        }

        #[tokio::test]
        async fn test_validate_install_package_no_tomls() {
            let temp = TempDir::new().unwrap();
            let wasm_path = temp.path().join("bare.wasm");
            fs::write(&wasm_path, vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]).unwrap();

            let package = validate_install_package(&wasm_path).await.unwrap();
            assert_eq!(package.wasm_path, wasm_path);
            assert!(package.component_toml_path.is_none());
            assert!(package.deployment_template_path.is_none());
        }

        #[tokio::test]
        async fn test_validate_install_package_invalid_extension() {
            let temp = TempDir::new().unwrap();
            let not_wasm = temp.path().join("fake.txt");
            fs::write(&not_wasm, b"nope").unwrap();

            let err = validate_install_package(&not_wasm).await.unwrap_err();
            assert!(err.to_string().contains("not have extension"));
        }
    }

    #[tokio::test]
    async fn test_prepare_install_package_in_temp_full() {
        let temp = TempDir::new().unwrap();
        let modules_dir = temp.path().join("modules");
        let temp_dir = temp.path().join("tmp");
        fs::create_dir_all(&modules_dir).unwrap();
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = StorageManager::new_for_tests(modules_dir, temp_dir);

        let source = TempDir::new().unwrap();
        let wasm = source.path().join("mod.wasm");
        let toml = source.path().join("mod.component.toml");
        fs::write(&wasm, vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]).unwrap();
        fs::write(&toml, "[component]\nname=\"mod\"\nversion=\"0.1.0\"").unwrap();

        let package = validate_install_package(&wasm).await.unwrap();
        let staged = prepare_install_package_in_temp(&storage, package).await.unwrap();

        assert!(staged.wasm_path.exists());
        assert!(staged.component_toml_path.unwrap().exists());
        assert_eq!(staged.wasm_path.file_name().unwrap(), "mod.wasm");
    }
}
