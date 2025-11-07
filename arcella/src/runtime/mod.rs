// arcella/arcella/src/runtime/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::{
    collections::{HashMap, HashSet},
    path::{PathBuf},
    sync::Arc,
    time::{Duration, Instant}
};
use time::OffsetDateTime;
use tokio::sync::{RwLock, broadcast};

use wasmtime::{
    Engine,
};

use ministate::StateManager;

use arcella_types::{
    manifest::ComponentManifest,
};

use crate::{storage, cache};
use crate::config::ArcellaConfig;
use crate::error::{ArcellaError, Result as ArcellaResult};
use crate::manifest::ComponentBundle;

mod state;
use state::*;

mod mutators;
use mutators::*;

struct ArcellaRuntimeEnvironment {
    pub pid: u32,
    pub start_instant: Instant,
    pub start_utc: OffsetDateTime,
}

pub struct ArcellaRuntimeStatus {
    pub pid: u32,
    pub start_time: OffsetDateTime,
    pub uptime: Duration,
}

pub struct ArcellaRuntime {
    pub config: Arc<ArcellaConfig>,
    pub storage: Arc<storage::StorageManager>,
    pub cache: Arc<cache::ModuleCache>,
    pub environment: Arc<RwLock<ArcellaRuntimeEnvironment>>,
    pub state_manager: Arc<StateManager<ArcellaState, ArcellaMutation>>,
}

impl ArcellaRuntime{
    pub async fn new(
        config: Arc<ArcellaConfig>,
        storage: Arc<storage::StorageManager>,
        cache: Arc<cache::ModuleCache>,
    ) -> ArcellaResult<Self> {

        let env = ArcellaRuntimeEnvironment {
            pid: std::process::id(),
            start_instant: Instant::now(),
            start_utc: OffsetDateTime::now_utc(),
        };

        let state_dir = storage.modules_dir.clone();
        let state_manager = StateManager::open(&state_dir, "arcella.wal.jsonl").await?;

        let runtime = Self {
            config,
            storage,
            cache,
            environment: Arc::new(RwLock::new(env)),
            state_manager: Arc::new(state_manager),
        };

        Ok(runtime)
    }

    pub async fn shutdown(&mut self) -> ArcellaResult<()> {
        // To be added stopping modules, instances, and the engine
        Ok(())
    }

    pub fn status(&self) -> ArcellaResult<ArcellaRuntimeStatus> {

        let env = self.environment.try_read().expect("Runtime environment poisoned");

        return Ok(ArcellaRuntimeStatus {
            pid: env.pid,
            start_time: env.start_utc,
            uptime: self.uptime(),
        });

    }

    pub fn uptime(&self) -> std::time::Duration {
        let env = self.environment.try_read().expect("Runtime environment poisoned");
        env.start_instant.elapsed()
    }

    pub async fn install_module_from_path(
        &mut self,
        wasm_path: &PathBuf,
    ) -> ArcellaResult<usize> {

        // 1. Валидация файла
        if !wasm_path.exists() {
            let error = ArcellaError::IoWithPath {
                source: std::io::ErrorKind::NotFound.into(),
                path: wasm_path.clone(),
            };
            tracing::error!("{}", error);
            return Err(error);
        }

        if wasm_path.extension().map_or(true, |ext| ext != "wasm") {
            let error = ArcellaError::RuntimeError(
                "Path is not a .wasm file".into(),
            );
            tracing::error!("{}", error);
            return Err(error);
        }        
        tracing::debug!("File {:?} is wasm", wasm_path );

        let engine = Engine::default();

        let bundle = match ComponentBundle::from_wasm_path(&engine, wasm_path) {
            Ok(boundle) => boundle,
            Err(e) => {
                tracing::error!("{}", e);
                return Err(e);
            }
        };

        let module_id = bundle.component.id();

        /*let mutator = InstallModule {

        };
        
        self.state_manager
            .apply(ArcellaMutation::InstallModule(mutator))
            .await?;*/

        tracing::debug!("Runtime: Installing module {:?}", module_id );

        Ok(10)
    }

    pub async fn deploy_module_from_path(
        &mut self,
        wasm_path: &PathBuf,
    ) -> ArcellaResult<usize> {

        tracing::debug!("Runtime: Deploing module from path: {:?}", wasm_path );

        Ok(10)
    }

    pub async fn module_start(
        &mut self,
        deployment_id: &str,
    ) -> ArcellaResult<String> {

        tracing::debug!("Runtime: Starting module {:?}", deployment_id );

        Ok(format!("Started"))
    }

    pub async fn module_stop(
        &mut self,
        deployment_id: &str,
    ) -> ArcellaResult<String> {

        tracing::debug!("Runtime: Stoping module {:?}", deployment_id );

        Ok(format!("Stoped"))
    }

    #[cfg(test)]
    pub async fn new_for_tests(config: Arc<ArcellaConfig>) -> ArcellaResult<Self> {

        let storage = Arc::new(storage::StorageManager::new(&config).await?);
        let cache = Arc::new(cache::ModuleCache::new(&config).await?);
        let test_runtime = Self::new(config, storage, cache).await?;

        Ok(test_runtime)
    }

}
