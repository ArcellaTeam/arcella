// arcella/arcella-core/src/storage/mod.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use crate::config::ArcellaConfig;
use crate::{ArcellaError, ArcellaResult};

pub struct StorageManager {
    pub cache_dir: PathBuf,
    pub metadata_dir: PathBuf,
    pub modules_dir: PathBuf,
    pub temp_dir: TempDir,
}

impl StorageManager {
    pub async fn new(
        config: &Arc<ArcellaConfig>,
    ) -> ArcellaResult<Self> {

        let cache_dir = config
            .base_dir.join(config.extract_path_value("cache.dir")?);
        tracing::debug!("Cache directory path: {:?}", cache_dir );

        let metadata_dir = config
            .base_dir.join(config.extract_path_value("metadata.dir")?);
        tracing::debug!("Metadata directory path: {:?}", metadata_dir );

        let modules_dir = config
            .base_dir.join(config.extract_path_value("modules.dir")?);
        tracing::debug!("Modules directory path: {:?}", modules_dir );

        let temp_dir = tempfile::tempdir()
            .map_err(|e| ArcellaError::IoWithPath {
                source: e,
                path: PathBuf::from("<tempdir>"),
            })?;
        tracing::debug!("Temporary directory created: {:?}", temp_dir.path());


        let manager = Self {
            cache_dir,
            metadata_dir,
            modules_dir,
            temp_dir,
        };

        manager.ensure_directories().await?;
        Ok(manager)

    }

    async fn ensure_directories(&self) -> ArcellaResult<()> {

        if !self.modules_dir.exists() {
            tokio::fs::create_dir_all(&self.modules_dir).await?;
            tracing::info!("Created modules directory: {:?}", self.modules_dir);
        }

        if !self.cache_dir.exists() {
            tokio::fs::create_dir_all(&self.cache_dir).await?;
            tracing::info!("Created cache directory: {:?}", self.cache_dir);
        }

        Ok(())
    } 

    pub fn temp_path(&self) -> &Path {
        self.temp_dir.path()
    }

    #[cfg(test)]
    pub fn new_for_tests(modules_dir: PathBuf, temp_dir: PathBuf) -> Self {
        use std::fs;
        // Создаём временный TempDir вручную из пути (trick для тестов)
        // Но проще — использовать реальный TempDir и переопределить пути
        // Однако для простоты: создаём все необходимые поддиректории
        let cache_dir = temp_dir.join("cache");
        let metadata_dir = temp_dir.join("metadata");

        fs::create_dir_all(&modules_dir).expect("Failed to create test modules dir");
        fs::create_dir_all(&cache_dir).expect("Failed to create test cache dir");
        fs::create_dir_all(&metadata_dir).expect("Failed to create test metadata dir");

        // Создаём отдельный TempDir для temp_dir менеджера
        let internal_temp = tempfile::tempdir().expect("Failed to create internal temp dir");

        Self {
            cache_dir,
            metadata_dir,
            modules_dir,
            temp_dir: internal_temp,
        }
    }
        
}
