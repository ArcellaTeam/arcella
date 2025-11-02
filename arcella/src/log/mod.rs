// arcella/arcella/src/log/mod.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Logging and tracing for the Arcella Runtime.
//!
//! This module implements a flexible, multi-channel logging system based on the [`tracing`] crate.
//! It supports three output channels:
//! - **File** (`arcella.log`) — with optional structured (JSON) or plain-text formatting;
//! - **stderr** — for convenient debugging when running in foreground mode;
//! - **In-memory ring buffer** — to expose recent logs via ALME (e.g., through the CLI).
//!
//! Configuration is read from `tracing.cfg` in Arcella’s config directory and allows:
//! - Setting a global log level;
//! - Configuring per-module or per-target log levels;
//! - Enabling/disabling individual output channels;
//! - Limiting the in-memory buffer size for ALME.
//!
//! The system is thread-safe and uses non-blocking I/O for file writes.

use std::collections::{VecDeque, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

use serde::{Deserialize, Deserializer};
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt,
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
    Layer,
};

use crate::error::{ArcellaError, Result as ArcellaResult};
use crate::config::{
    ARCELLA_PREFIX,
    ArcellaConfig,
    extract_subtree,
};

// Global resources for logger

/// In-memory ring buffer storing the most recent log entries.
/// Used to serve logs via ALME (e.g., for CLI queries).
// TODO: For high-load tasks, it is necessary to implement 
// a lock-free circular buffer with multi-reader snapshots
static LOG_BUFFER: std::sync::OnceLock<Arc<Mutex<VecDeque<String>>>> = std::sync::OnceLock::new();

fn get_log_buffer() -> Option<&'static Arc<Mutex<VecDeque<String>>>> {
    LOG_BUFFER.get()
}

/// Helper: deserialize LevelFilter from string (e.g., "info", "debug")
fn deserialize_level_filter<'de, D>(deserializer: D) -> Result<LevelFilter, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<LevelFilter>().map_err(serde::de::Error::custom)
}

/// Helper: deserialize a map of per-module log levels.
fn deserialize_module_levels<'de, D>(deserializer: D) -> Result<HashMap<String, LevelFilter>, D::Error>
where
    D: Deserializer<'de>,
{
    let map: HashMap<String, String> = Deserialize::deserialize(deserializer)?;
    let mut result = HashMap::new();
    for (target, level_str) in map {
        let level = level_str
            .parse::<LevelFilter>()
            .map_err(serde::de::Error::custom)?;
        result.insert(target, level);
    }
    Ok(result)
}

/// Arcella tracing configuration.
#[derive(Deserialize, Debug, Clone)]
pub struct TracingConfig {
    /// Default log level for targets under the `arcella` namespace.
    ///
    /// Valid values: `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"`.
    #[serde(default = "default_log_level", deserialize_with = "deserialize_level_filter")]
    pub default_level: LevelFilter,

    /// Use structured (JSON) format for file logging.
    ///
    /// If `true`, entries in `arcella.log` will be JSON-encoded, suitable for machine processing
    /// (e.g., ingestion into ELK or Loki). If `false`, human-readable text is used.
    #[serde(default = "default_structured")]
    pub structured: bool,

    #[serde(default = "default_dir")]
    pub dir: PathBuf,

    /// Output logs to stderr.
    ///
    /// Useful when running Arcella in foreground mode or inside a container where stderr is
    /// captured by an orchestrator (e.g., systemd or Kubernetes).
    #[serde(default = "default_stderr")]
    pub stderr: bool,

    /// Write logs to the `arcella.log` file.
    ///
    /// The file is created in the directory specified by `ArcellaConfig::log_dir`.
    #[serde(default = "default_file")]
    pub file: bool,

    /// Maximum size of the in-memory ring buffer for ALME (in log entries).
    ///
    /// A value of `0` disables the buffer. Used by the `alme logs` command to retrieve
    /// the most recent `N` lines without reading the log file.
    #[serde(default = "default_alme_buffer_size")]
    pub alme_buffer_size: usize,

    /// Per-module/target log levels.
    ///
    /// Keys are target names (e.g., `"arcella::runtime"`), values are log levels.
    /// These override `default_level` for the specified targets.
    ///
    /// Example:
    /// ```toml
    /// [modules]
    /// "arcella::runtime" = "info"
    /// "arcella::alme" = "debug"
    /// ```
    #[serde(default, deserialize_with = "deserialize_module_levels")]
    pub internal: HashMap<String, LevelFilter>,
}

fn default_log_level() -> LevelFilter { LevelFilter::INFO }
fn default_structured() -> bool { false }
fn default_stderr() -> bool { true }
fn default_file() -> bool { true }
fn default_alme_buffer_size() -> usize { 100 }
fn default_dir() -> PathBuf { PathBuf::from("dir") }

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            default_level: default_log_level(),
            structured: default_structured(),
            dir: default_dir(),
            stderr: default_stderr(),
            file: default_file(),
            alme_buffer_size: default_alme_buffer_size(),
            internal: HashMap::new(),
        }
    }
}

// === Layer for ALME ===

/// A tracing layer that writes events to an in-memory ring buffer for later retrieval via ALME.
struct AlmeBufferLayer {
    max_size: usize,
}

impl AlmeBufferLayer {
    fn new(max_size: usize) -> Self {
        Self { max_size }
    }
}

impl<S> Layer<S> for AlmeBufferLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if let Some(buffer) = get_log_buffer() {
            let meta = event.metadata();

            let now: OffsetDateTime = OffsetDateTime::now_utc();
            let now_rfc3339 = now.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "<invalid-timestamp>".to_string());

            let mut visitor = EventVisitor::default();
            event.record(&mut visitor);

            let message = if visitor.message.is_empty() {
                "no message".to_string()
            } else {
                visitor.message
            };

            let fields = if visitor.fields.is_empty() {
                String::new()
            } else {
                format!(" {{{}}}", visitor.fields.join(", "))
            };

            let line = format!(
                "{} {} {}: {}{}",
                now_rfc3339,
                meta.level(),
                meta.target(),
                message,
                fields
            ); 

            match buffer.lock() {
                Ok(mut buf) => {
                    if buf.len() >= self.max_size {
                        buf.pop_front();
                    }
                    buf.push_back(line);
                }
                Err(e) => {
                    // Avoid panicking in a tracing handler; silently ignore if poisoned
                    eprintln!("ALME log buffer poisoned: {}", e);
                }
            }

        }
    }
}

/// Returns up to `n` most recent log entries from the in-memory buffer.
///
/// The buffer is only populated if `alme_buffer_size > 0` in `tracing.cfg`.
/// Entries are returned in reverse chronological order (most recent first).
///
/// # Arguments
///
/// * `n` — maximum number of log lines to return.
///
/// # Returns
///
/// A vector of log strings. Returns an empty vector if the buffer is uninitialized or disabled.
pub fn get_recent_logs(n: usize) -> Vec<String> {
    if let Some(buffer) = get_log_buffer() {
        match buffer.lock() {
            Ok(buf) => {
                buf.iter().rev().take(n).cloned().collect()
            }
            Err(e) => {
                eprintln!("Failed to lock ALME log buffer: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    }
}

#[derive(Default)]
struct EventVisitor {
    message: String,
    fields: Vec<String>,
}

impl tracing::field::Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else {
            self.fields.push(format!("{}={:?}", field.name(), value));
        }
    }
}

/// Initializes the global `tracing` subscriber based on the provided configuration.
///
/// This function must be called exactly once during daemon startup. It:
/// - Creates the log directory if it doesn’t exist;
/// - Loads or falls back to default settings from `tracing.cfg`;
/// - Configures logging layers: file, stderr, and in-memory buffer for ALME;
/// - Installs a global subscriber for `tracing`.
///
/// # Arguments
///
/// * `config` — reference to the main Arcella configuration, which includes paths to logs and config files.
///
/// # Returns
///
/// A `WorkerGuard` from `tracing_appender`, which must be kept alive until shutdown
/// to ensure buffered log entries are flushed to disk. Returns `None` if file logging is disabled.
											  
///
/// # Errors
///
/// Returns an error if:
/// - Failed to parse log config:;
/// - The global subscriber has already been initialized;
/// - The ALME in-memory buffer fails to initialize (when enabled).
pub fn init(config: &ArcellaConfig) -> ArcellaResult<Option<tracing_appender::non_blocking::WorkerGuard>> {

    let mut file_guard: Option<tracing_appender::non_blocking::WorkerGuard> = None;

    // === Извлекаем поддерево arcella.log ===
    let log_table = extract_subtree(&config.config_values, &(ARCELLA_PREFIX.to_owned() + "log"));    
    let tracing_cfg: TracingConfig = toml::Value::Table(log_table)
        .try_into()
        .map_err(|e| ArcellaError::Config(format!("failed to parse log config: {}", e)))?;

    // Ensure log directory exists
    let log_dir = config.base_dir.join(&tracing_cfg.dir);
    fs::create_dir_all(&log_dir)
        .map_err(|e| ArcellaError::Io(e))?;

    // Initialize ALME in-memory buffer
    if tracing_cfg.alme_buffer_size > 0 {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(tracing_cfg.alme_buffer_size)));
        LOG_BUFFER.set(buffer).map_err(|_| ArcellaError::Internal("LOG_BUFFER already set".into()))?;
    }

    // Build filter directives
    let mut directives = vec![format!("arcella={}", tracing_cfg.default_level)];

    // Override per-module levels
    for (target, level) in &tracing_cfg.internal {
        directives.push(format!("{}={}", target, level));
    }

    let filter = directives.join(",");

    let env_filter = EnvFilter::try_new(filter)
        .map_err(|e| ArcellaError::Config(format!("invalid log filter: {}", e)))?;

    let mut layers = Vec::new();

    // 1. File layer
    if tracing_cfg.file {

        // Use `never` rolling (single file: arcella.log)
        let file_appender = tracing_appender::rolling::never(&log_dir, "arcella.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = if tracing_cfg.structured {
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_ansi(false)
                .boxed()
        } else {
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .boxed()
        };
        layers.push(file_layer);
        file_guard = Some(guard);
    }

    // 2. StdErr layer
    if tracing_cfg.stderr {
        let console_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .boxed();
        layers.push(console_layer);
    }

    // 3. In-memory ALME layer
    if tracing_cfg.alme_buffer_size > 0 {
        let alme_layer = AlmeBufferLayer::new(tracing_cfg.alme_buffer_size);
        layers.push(Box::new(alme_layer));
    }    

    let subscriber = tracing_subscriber::registry()
        .with(layers)
        .with(env_filter);

    subscriber
        .try_init()
        .map_err(|e| ArcellaError::Internal(format!("failed to init tracing: {}", e)))?;


    Ok(file_guard)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArcellaConfig, IntegrityChecker};
    use arcella_types::config::{ConfigValues, Value as TomlValue};
    use indexmap::IndexMap;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tracing::Level;

    fn make_toml_value(s: &str) -> TomlValue {
        TomlValue::String(s.to_string())
    }

    fn make_toml_bool(b: bool) -> TomlValue {
        TomlValue::Boolean(b)
    }

    fn make_toml_int(i: i64) -> TomlValue {
        TomlValue::Integer(i)
    }

    #[tokio::test]
    async fn test_init_with_default_config_only() {
        // 1. Создаём временный каталог
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base_dir = temp_dir.path().to_path_buf();
        let config_dir = base_dir.join("config");
        let log_dir = base_dir.join("log");
        let modules_dir = base_dir.join("modules");
        let cache_dir = base_dir.join("cache");
        let socket_path = base_dir.join("alme.sock");

        // 2. Воссоздаём config_values как из default_config.toml
        // Предположим, что в default_config.toml есть:
        // arcella.log.level = "info"
        // arcella.log.stderr = true
        // arcella.log.file = true
        // arcella.log.structured = false
        // arcella.log.alme_buffer_size = 100
        let mut config_values: ConfigValues = IndexMap::new();
        config_values.insert("arcella.log.level".to_string(), (make_toml_value("info"), 0));
        config_values.insert("arcella.log.dir".to_string(), (make_toml_value("log"), 0));
        config_values.insert("arcella.log.stderr".to_string(), (make_toml_bool(true), 0));
        config_values.insert("arcella.log.file".to_string(), (make_toml_bool(true), 0));
        config_values.insert("arcella.log.structured".to_string(), (make_toml_bool(false), 0));
        config_values.insert("arcella.log.alme_buffer_size".to_string(), (make_toml_int(100), 0));

        // Также должны быть заданы обязательные пути
        config_values.insert("arcella.modules.dir".to_string(), (make_toml_value(modules_dir.to_str().unwrap()), 0));
        config_values.insert("arcella.cache.dir".to_string(), (make_toml_value(cache_dir.to_str().unwrap()), 0));
        config_values.insert("arcella.alme.socket_path".to_string(), (make_toml_value(socket_path.to_str().unwrap()), 0));

        // 3. Создаём пустой IntegrityChecker
        let integrity_checker = IntegrityChecker::new(vec![]).expect("Failed to create empty IntegrityChecker");

        // 4. Создаём фиктивный ArcellaConfig
        let config = ArcellaConfig {
            config_values,
            base_dir,
            config_dir,
            modules_dir,
            cache_dir,
            integrity_checker,
        };

        // 4. Инициализируем логирование
        let _guard = init(&config).expect("log::init should succeed");

        // 5. Проверяем, что подсистема tracing работает: пишем тестовое сообщение
        tracing::info!("Test log message from default config");

        // 6. Проверяем, что буфер ALME содержит сообщение
        let recent_logs = get_recent_logs(10);
        assert!(!recent_logs.is_empty(), "ALME log buffer should contain logs");
        assert!(recent_logs.iter().any(|line| line.contains("Test log message from default config")));

        // 7. Проверяем, что лог-файл создан и содержит сообщение
        let log_file_path = log_dir.join("arcella.log");
        assert!(log_file_path.exists(), "arcella.log should be created");

        let log_content = std::fs::read_to_string(&log_file_path)
            .expect("Failed to read arcella.log");
        assert!(log_content.contains("Test log message from default config"));

        // 8. Дополнительно: проверим, что уровень "debug" не попал в лог
        tracing::debug!("This debug message should not appear in logs");
        let recent_logs_after_debug = get_recent_logs(10);
        // Поскольку уровень = info, debug-сообщение не должно быть записано
        assert!(!recent_logs_after_debug.iter().any(|line| line.contains("This debug message")));

        // Тест завершён; TempDir удалится автоматически
    }
}
