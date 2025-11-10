// arcella/src/runtime/deploy.rs
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
pub struct DeployPackage {
    /// Base directory containing all package files (e.g., staging dir or original dir).
    pub package_dir: Option<PathBuf>,

    /// Path to the `.deployment.toml` file inside `package_dir`.
    pub deployment_toml_path: PathBuf,

}

pub async fn validate_deploy_package(deploy_path: &Path) -> ArcellaResult<DeployPackage> {
    // 1. Проверка: это файл?
    if !deploy_path.is_file() {
        return Err(ArcellaError::InvalidArgument{
            message: format!("Path is not a file: {:?}", deploy_path),
        });
    }

    // 2. Проверка расширения: должно заканчиваться на `.deployment.toml`
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

    // 3. Проверка: есть ли осмысленное имя перед `.deployment.toml`?
    let base_name = file_name.trim_end_matches(".deployment.toml");
    if base_name.is_empty() {
        return Err(ArcellaError::InvalidArgument {
            message: "Deployment filename must have a non-empty prefix (e.g., 'web.deployment.toml')".into(),
        });
    }

    // 4. Опционально: проверка допустимых символов в имени (для будущих deployment_id)
    if !base_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ArcellaError::InvalidArgument {
            message: "Deployment name must contain only alphanumeric, '-', or '_' characters".into(),
        });
    }

    Ok(DeployPackage {
        package_dir: None,
        deployment_toml_path: deploy_path.to_path_buf(),
    })

}

pub async fn prepare_deploy_package_in_temp(
    storage: &StorageManager,
    package: DeployPackage,
) -> ArcellaResult<DeployPackage> {
    let staging_dir = tempfile::tempdir_in(storage.temp_path())
        .map_err(|e| ArcellaError::IoWithPath {
            source: e,
            path: storage.temp_path().to_path_buf(),
        })?
        .keep(); // Передаём владение в PathBuf

    // Копирование .deployment.toml
    let deployment_toml_path = package.deployment_toml_path
        .file_name()
        .ok_or_else(|| ArcellaError::InvalidArgument { 
            message: "'.deployment.toml' file has no name".into()
        })?;
    let staged_deployment_toml = staging_dir.join(deployment_toml_path);
    tokio::fs::copy(&package.deployment_toml_path, &staged_deployment_toml).await
        .map_err(|e| ArcellaError::IoWithPath {
            source: e,
            path: package.deployment_toml_path.clone(),
        })?;

    Ok(DeployPackage {
        package_dir: Some(staging_dir.clone()),
        deployment_toml_path: staged_deployment_toml,
    })
}
