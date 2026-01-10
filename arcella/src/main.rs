// arcella/arcella/src/main.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;

use arcella_core::{
    cache,
    config,
    runtime,
    storage,
    ArcellaError,
    ArcellaResult,
};    
    
mod alme;
mod log;

/// Arcella: Modular WebAssembly Runtime
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {}

#[tokio::main]
async fn main() -> ArcellaResult<()> {

    // 1. Load configuration (e.g., paths, runtime options)
    let _ = Cli::parse(); 
    let (config_data, warnings) = config::load().await?;
    let config = Arc::new(config_data);

    // 2. Initialize logging (should be the first side effect)
    let log_guard: Option<tracing_appender::non_blocking::WorkerGuard> = log::init(&config)?;
    tracing::info!("Starting up (v{})", env!("CARGO_PKG_VERSION"));

    // 3. Log warnings from config loading
    for warning in warnings {
        tracing::warn!("{}", warning);
    }

    // 4. Initialize core subsystems: storage and module cache
    let storage = match storage::StorageManager::new(&config).await {
        Ok(storage) => storage,
        Err(e) => {
            tracing::error!("Failed to initialize storage: {}", e);
            return Err(e);
        }
    };
    let storage = Arc::new(storage);
    tracing::debug!("Initialize storage");

    let cache = Arc::new(cache::ModuleCache::new(&config).await?);
    tracing::debug!("Initialize cache");

    let runtime = match runtime::ArcellaRuntime::new(config.clone(), storage.clone(), cache.clone()).await {
        Ok(runtime) => runtime,
        Err(e) => {
            tracing::error!("Failed to initialize runtime: {}", e);
            return Err(e);
        }
    };
    let runtime = Arc::new(RwLock::new(runtime));
    tracing::debug!("Initialize core runtime");

    let alme_handle = match alme::start(runtime.clone()).await {
        Ok(handle) => handle,
        Err(e) => {
            tracing::error!("Failed to start ALME: {}", e);
            return Err(e);
        }
    };
    tracing::info!("Starting ALME server");

    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            tracing::info!("Received Ctrl+C");
        },
        Err(e) => {
            tracing::error!("Failed to listen for Ctrl+C: {}", e);
            return Err(ArcellaError::Internal(e.to_string()));
        }
    };
    tracing::info!("Received Ctrl+C, shutting down...");

    if let Err(e) = runtime.write().await.shutdown().await {
        tracing::error!("Runtime shutdown error: {}", e);
    }
    if let Err(e) = alme_handle.shutdown().await {
        tracing::error!("ALME shutdown error: {}", e);
    }

    tracing::info!("Shutting down");
        
    // Configure the engine
    /*let mut config = Config::default();
    config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
    config.wasm_multi_memory(false);
    config.wasm_threads(false);
    config.consume_fuel(true);

    // Initialize the engine
    let engine = Engine::new(&config)?;
    let mut linker: Linker<p1::WasiP1Ctx> = Linker::new(&engine);

    p1::add_to_linker_sync(&mut linker, |t| t)?;
    let wasi_ctx = WasiCtxBuilder::new()
        .inherit_stderr()
        .inherit_stdout()
        .build_p1();

    let mut store = Store::new(&engine, wasi_ctx);
    let _ = store.set_fuel(1_000_000);

    // Load the module

    let module_bytes = load_module_bytes(&cli.module)?;
    let module = Module::from_binary(&engine, &module_bytes)
        .map_err(|e| anyhow!("Failed to compile module: {}", e))?;

    linker.module(&mut store, "default", &module)?;

    match linker.get_default(&mut store, "default") {
        Ok(func) => {
            if let Err(e) = func.typed::<(), ()>(&store)?.call(&mut store, ()) {
                if e.is::<Trap>() {
                    eprintln!("WASM module exited with trap: {}", e);
                } else {
                    return Err(e.into());
                }
            }
        }
        Err(_) => {
            eprintln!("No default function found — nothing to run.");
        }
    }*/

    drop(log_guard);

    Ok(())
    
}

/*fn load_module_bytes(path: &PathBuf) -> ArcellaResult<Vec<u8>> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| ArcellaError::InvalidModulePath(path.clone()))?;

    match extension {
        "wat" => {
            let wat_content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read .wat file: '{}'", path.display()))?;
            wat::parse_str(&wat_content)
                .with_context(|| format!("Failed to parse .wat file: '{}'", path.display()))
        }
        "wasm" => {
            std::fs::read(path)
                .with_context(|| format!("Failed to read .wasm file: '{}'", path.display()))
        }
        _ => Err(anyhow::anyhow!(
            "Unsupported file type: '{}'. Only .wat and .wasm are supported.",
            path.display()
        )),
    }
}*/