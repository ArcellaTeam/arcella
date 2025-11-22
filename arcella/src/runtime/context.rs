// arcella/src/runtime/context.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use wasmtime::Engine;

use crate::{
    cache::ModuleCache,
    config::ArcellaConfig,
    runtime::{
        state::ArcellaState,
        ArcellaMutation,
    },
    storage::StorageManager,
    ArcellaResult,
};

use ministate::StateManager;
use arcella_types::manifest::ModuleId;

type InstallLocks = Mutex<std::collections::HashMap<ModuleId, Arc<Mutex<()>>>>;

/// Execution context capturing all shared dependencies needed to perform
/// lifecycle operations (install, deploy, start, stop, etc.) without
/// holding a reference to the main ArcellaRuntime.
#[derive(Clone)]
pub struct ArcellaExecutionContext {
    pub config: Arc<ArcellaConfig>,
    pub storage: Arc<StorageManager>,
    pub cache: Arc<ModuleCache>,
    pub engine: Engine,
    pub state_manager: Arc<StateManager<ArcellaState, ArcellaMutation>>,
    pub install_locks: Arc<InstallLocks>,
}

impl ArcellaExecutionContext {
    pub async fn from_runtime(
        runtime: &Arc<RwLock<super::ArcellaRuntime>>,
    ) -> ArcellaResult<Self> {
        let runtime_guard = runtime.read().await;

        Ok(ArcellaExecutionContext {
            config: runtime_guard.config.clone(),
            storage: runtime_guard.storage.clone(),
            cache: runtime_guard.cache.clone(),
            engine: runtime_guard.engine.clone(),
            state_manager: runtime_guard.state_manager.clone(),
            install_locks: runtime_guard.install_locks.clone(),
        })
    }
}