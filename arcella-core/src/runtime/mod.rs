// arcella/arcella-core/src/runtime/mod.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::{
    collections::HashMap,
    path::{PathBuf},
    sync::Arc,
    time::{Duration, Instant}
};
use time::OffsetDateTime;
use tokio::{
    fs,
    sync::{
        Mutex,
        RwLock,
    }
};

use ministate::StateManager;

use arcella_types::module_id::ModuleId;

use crate::{
    ArcellaError,
    ArcellaResult,
    cache,
    config::ArcellaConfig,
    manifest::{
        ComponentBundle,
        DeploymentSpec,
    },
    storage,
};

mod state;
use state::*;

mod context;
pub use context::*;

mod mutators;
use mutators::*;

mod install;
use install::*;

mod deploy;
use deploy::*;

pub struct ArcellaRuntimeEnvironment {
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
    pub engine: wasmtime::Engine,
    pub install_locks: Arc<Mutex<HashMap<ModuleId, Arc<Mutex<()>>>>>,
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

        let metadata_dir = storage.metadata_dir.clone();
        let state_manager = StateManager::open(&metadata_dir, "arcella.wal.jsonl").await?;

        let engine = match wasmtime::Engine::new(&wasmtime::Config::new().async_support(true)) {
            Ok(engine) => engine,
            Err(e) => {
                tracing::error!("Failed to create Wasmtime engine: {}", e);
                return Err(ArcellaError::WasmtimeError(e));
            }
        };

        let runtime = Self {
            config,
            storage,
            cache,
            engine,
            install_locks: Arc::new(Mutex::new(HashMap::new())),
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
        ctx: ArcellaExecutionContext,
        wasm_path: &PathBuf,
    ) -> ArcellaResult<ModuleId> {
        tracing::info!("Starting installation from: {:?}", wasm_path);

        // 1. Validate and stage input package structure
        let validated = validate_install_package(wasm_path).await?;
        tracing::debug!("Package validated: wasm={:?}", validated.wasm_path);
        let staged = prepare_install_package_in_temp(&ctx.storage, validated).await?;
        tracing::debug!("Package staged to: {:?}", staged.package_dir);   

        // 2. Parse bundle
        let bundle = if let Some(ref toml) = staged.component_toml_path {
            match ComponentBundle::from_wasm_and_toml(&ctx.engine, &staged.wasm_path, toml) {
                Ok(bundle) => bundle,
                Err(e) => {
                    tracing::error!("Failed to parse component manifest: {}", e);
                    return Err(e);
                }
            }
        } else {
            match ComponentBundle::from_wasm_path(&ctx.engine, &staged.wasm_path) {
                Ok(bundle) => bundle,
                Err(e) => {
                    tracing::error!("Failed to parse component manifest: {}", e);
                    return Err(e);
                }
            }
        };
        let module_id = bundle.component.id.clone();
        tracing::info!("Parsed module ID: {}", module_id);

        // 3. Fine-grained lock per module_id
        let module_lock = {
            let mut locks = ctx.install_locks.lock().await;
            locks.entry(module_id.clone()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
        };
        let _guard = module_lock.lock().await;        

        // 4. Check for duplicates (state + disk)
        let current_state = ctx.state_manager.snapshot().await;
        check_module_not_installed(
            &current_state,
            &ctx.storage.modules_dir,
            &module_id,
        ).await?;
        tracing::debug!("Module ID is unique");

        // 5. Install files to permanent storage
        install_module_files_to_storage(
            &staged,
            &ctx.storage.modules_dir,
            &module_id,
        ).await?;
        tracing::debug!("Files installed to modules directory");

        // 6. Record in WAL state
        let mutator = InstallModule {
            manifest: bundle.component.clone(),
        };
        match ctx.state_manager
            .apply(ArcellaMutation::InstallModule(mutator))
            .await {
                Ok(_) => (),
                Err(e) => {
                    tracing::error!("Failed to record module installation: {}", e);
                    return Err(e.into());
                }
            };
        tracing::info!("Module installed and recorded in state: {}", module_id);

        // 7. Cleanup staging directory
        if let Some(ref staging_dir) = staged.package_dir {
            fs::remove_dir_all(staging_dir).await.ok();
            tracing::debug!("Staging directory cleaned up");
        }

        Ok(module_id)
    }

    pub async fn deploy_module_from_path(
        ctx: ArcellaExecutionContext,
        deploy_path: &PathBuf,
    ) -> ArcellaResult<(String, String)> {

        tracing::info!("Starting deploy from: {:?}", deploy_path);

        // 1. Validate input package structure
        let validated = validate_deploy_package(deploy_path).await?;
        tracing::debug!("Deployment package {:?} validated", deploy_path);

        // 2. Stage into anonymous temp directory
        let state = ctx.state_manager.snapshot().await;
        let staged = prepare_deploy_package_in_temp(&ctx.storage, &state, validated).await?;
        tracing::debug!("Deployment staged to: {:?}", staged.package_dir);   

        // 3. Parse deployment specification
        let spec = DeploymentSpec::from_file(&staged.deployment_toml_path)?;
        tracing::info!("Parsed deployment spec: module_id={}, group={}", spec.module_id, spec.group);

        Ok(("module_id".to_string(), "deploy_id".to_string()))
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

        tracing::debug!("Runtime: Stopping module {:?}", deployment_id );

        Ok(format!("Stopped"))
    }

    #[cfg(test)]
    pub async fn new_for_tests(config: Arc<ArcellaConfig>) -> ArcellaResult<Self> {

        let storage = Arc::new(storage::StorageManager::new(&config).await?);
        let cache = Arc::new(cache::ModuleCache::new(&config).await?);
        let test_runtime = Self::new(config, storage, cache).await?;

        Ok(test_runtime)
    }

}
