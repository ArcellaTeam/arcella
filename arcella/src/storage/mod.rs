// arcella/arcella/src/storage/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;
use std::path::PathBuf;

use crate::config::ArcellaConfig;
use crate::error::{ArcellaError, Result as ArcellaResult};

pub struct StorageManager {
    pub modules_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl StorageManager {
    pub async fn new(
        config: &Arc<ArcellaConfig>,
    ) -> ArcellaResult<Self> {

        let modules_dir = config.extract_path_value("modules.dir")?;
        let cache_dir = config.extract_path_value("cache.dir")?;

        let modules_dir = config.base_dir.join(modules_dir);
        let cache_dir = config.base_dir.join(cache_dir);

        tracing::debug!("Modules directory path: {:?}", modules_dir );
        tracing::debug!("Cache directory path: {:?}", cache_dir );

        let manager = Self {
            modules_dir,
            cache_dir,
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

}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /*#[tokio::test]
    async fn test_storage_manager_creates_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().join("arcella_test");

        let config = Arc::new(ArcellaConfig {
            base_dir: Some(base_path.clone()),
            config_dir: Some(base_path.join("config")),
            log_dir: Some(base_path.join("log")),
            modules_dir: Some(base_path.join("modules")),
            cache_dir: Some(base_path.join("cache")),
            socket_path: Some(base_path.join("alme")),
        });

        let storage = StorageManager::new(&config).await.unwrap();

        assert!(storage.base_dir.exists());
        assert!(storage.config_dir.exists());
        assert!(storage.modules_dir.exists());
        assert!(storage.cache_dir.exists());

        // Проверка прав доступа для base_dir (только на Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&storage.base_dir).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o700);
        }
    }*/
}