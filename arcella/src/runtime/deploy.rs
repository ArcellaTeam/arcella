// arcella/src/runtime/deploy.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::{Path, PathBuf};

use arcella_fs_utils::{
    create_temp_subdir,
    copy_files_to_dir,
    sync_directory,
    atomic_rename,
};

use crate::{
    ArcellaError,
    ArcellaResult,
    manifest::{DeploymentSpec, validate_module_id},
    runtime::state::ArcellaState,
    storage::StorageManager,
};

/// Represents a validated module package ready for installation.
#[derive(Debug, Clone)]
pub struct DeployPackage {
    /// Base directory containing all package files (e.g., staging dir or original dir).
    pub package_dir: Option<PathBuf>,

    /// Path to the `.deployment.toml` file inside `package_dir`.
    pub deployment_toml_path: PathBuf,

    /// Extracted deployment ID (e.g., "web" from "web.deployment.toml").
    pub deployment_id: String,

}

impl DeployPackage {
    /// Validates that the deployment file exists and parses as a `DeploymentSpec`.
    pub async fn parse_spec(&self) -> ArcellaResult<DeploymentSpec> {
        DeploymentSpec::from_file(&self.deployment_toml_path)
    }

    /// Returns all existing file paths as owned `PathBuf`s.
    ///
    /// Only includes paths that are `Some`. The `.wasm` path is always included.
    /// Order is deterministic but should not be relied upon externally.
    pub fn existing_file_paths_owned(&self) -> Vec<PathBuf> {
        let mut paths = Vec::with_capacity(1);
        paths.push(self.deployment_toml_path.clone());

        paths
    }    

    /// Reconstructs a `DeployPackage` from a staging directory.
    ///
    /// Assumes the deployment file has been copied into `staging_dir` with its original name.
    pub fn from_staging_dir(&self, staging_dir: PathBuf) -> ArcellaResult<Self> {
        let staged_path = staging_dir.join(
            self.deployment_toml_path
                .file_name()
                .ok_or_else(|| ArcellaError::InvalidArgument {
                    message: "Deployment file has no name".into(),
                })?,
        );

        if !staged_path.exists() {
            return Err(ArcellaError::InvalidArgument {
                message: format!("Staged deployment file not found: {:?}", staged_path),
            });
        }

        Ok(DeployPackage {
            package_dir: Some(staging_dir),
            deployment_toml_path: staged_path,
            deployment_id: self.deployment_id.clone(),
        })
    }

}

/// Validates a user-provided `.deployment.toml` path.
///
/// Checks:
/// - File exists and has `.deployment.toml` extension,
/// - Prefix before `.deployment.toml` is a valid deployment ID,
/// - Referenced `module_id` is syntactically valid.
///
/// Does **not** check if the module is installed — that happens later.
pub async fn validate_deploy_package(deploy_path: &Path) -> ArcellaResult<DeployPackage> {
    if !deploy_path.is_file() {
        return Err(ArcellaError::InvalidArgument {
            message: format!("Path is not a file: {:?}", deploy_path),
        });
    }

    // Check extension: must end with `.deployment.toml`
    let file_name = deploy_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ArcellaError::InvalidArgument {
            message: "Invalid or non-UTF-8 filename".into(),
        })?;

    if !file_name.ends_with(".deployment.toml") {
        return Err(ArcellaError::InvalidArgument {
            message: format!(
                "Deployment file must have '.deployment.toml' extension (e.g., 'web.deployment.toml'): {:?}",
                deploy_path
            ),
        });
    }

    // Check prefix: must be a valid deployment ID
    let deployment_id = file_name.trim_end_matches(".deployment.toml");
    if deployment_id.is_empty() {
        return Err(ArcellaError::InvalidArgument {
            message: "Deployment filename must have a non-empty prefix (e.g., 'web.deployment.toml')".into(),
        });
    }

    // Validate deployment_id format (alphanumeric + safe chars)
    if !deployment_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ArcellaError::InvalidArgument {
            message: "Deployment ID name must contain only alphanumeric, '-', or '_' characters".into(),
        });
    }

    Ok(DeployPackage {
        package_dir: None,
        deployment_toml_path: deploy_path.to_path_buf(),
        deployment_id: deployment_id.to_string(),
    })

}

/// Stages the deployment file into a temporary directory and validates its contents.
///
/// Also ensures that the referenced module is **already installed** (in state or on disk).
pub async fn prepare_deploy_package_in_temp(
    storage: &StorageManager,
    state: &ArcellaState,
    package: DeployPackage,
) -> ArcellaResult<DeployPackage> {
    // Parse spec early to get module_id
    let spec = package.parse_spec().await?;
    if !validate_module_id(&spec.module_id) {
        return Err(ArcellaError::InvalidArgument {
            message: format!("Invalid module_id in deployment: {}", spec.module_id),
        });
    }

    // Ensure module is installed
    if !state.installed_modules.contains_key(&spec.module_id) {
        let e = ArcellaError::ModuleNotInstalled(spec.module_id.clone());
        tracing::warn!("{}", e);
        return Err(e);
    }

    // Stage file
    let staging_dir = create_temp_subdir(storage.temp_path(), Some("deploy-staging")).await?;
    let source_files = package.existing_file_paths_owned();
    let _ = copy_files_to_dir(&source_files, &staging_dir, false).await?;

    package.from_staging_dir(staging_dir)
}
