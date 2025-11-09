// arcella/arcella-cli/src/main.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use arcella_types::alme::proto::{AlmeCommand, AlmeRequest, AlmeResponse};

/// Arcella CLI — управление runtime'ом через ALME
#[derive(Parser)]
#[command(version, about = "Arcella CLI — управление через ALME", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Проверить доступность ALME
    Ping,

    /// Вывести последние N строк лога
    #[command(name = "log:tail")]
    LogTail {
        /// Количество строк (по умолчанию: 100)
        #[arg(short, long, default_value_t = 100)]
        n: usize,
    },

    /// Получить статус runtime'а
    Status,

    /// Список установленных модулей
    #[command(name = "module:list")]
    ModuleList,

    /// Установить модуль из .wasm-файла
    #[command(name = "module:install")]
    ModuleInstall {
        /// Путь к .wasm-файлу или директории с компонентом
        path: PathBuf,
    },

    /// Развернуть модуль по спецификации
    #[command(name = "module:deploy")]
    ModuleDeploy {
        /// Путь к .deployment.toml файлу
        file: PathBuf,
    },

    /// Запустить развёртывание
    #[command(name = "module:start")]
    ModuleStart {
        /// Идентификатор развёртывания (например, web-http-logger-v1)
        deployment_id: String,
    },

    /// Остановить развёртывание
    #[command(name = "module:stop")]
    ModuleStop {
        /// Идентификатор развёртывания
        deployment_id: String,
    },

    /// Интерактивная консоль
    Shell,
}

async fn send_alme_request(
    socket_path: &PathBuf,
    request: AlmeRequest,
) -> anyhow::Result<AlmeResponse> {
    let stream = match UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "ALME socket not found at {}. Is Arcella running?",
                socket_path.display()
            );
        }
        Err(e) => {
            anyhow::bail!("Failed to connect to ALME server: {}", e);
        }
    };
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    let request_json = serde_json::to_vec(&request)?;
    writer.write_all(&request_json).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;

    if response_line.is_empty() {
        anyhow::bail!("ALME server closed connection unexpectedly");
    }

    let response: AlmeResponse = serde_json::from_str(&response_line)?;
    Ok(response)
}

fn get_default_socket_path() -> PathBuf {
    let base = dirs::home_dir().unwrap().join(".arcella");
    base.join("alme")
}

async fn exec_alme_cmd<F>(
    socket_path: &PathBuf,
    build_request: F,
    success_msg: &str,
) -> anyhow::Result<()>
where
    F: Fn() -> AlmeRequest,
{
    let req = build_request();
    let resp = send_alme_request(socket_path, req).await?;
    if resp.success {
        if success_msg != "" {
            println!("{}", success_msg);
        }
        if resp.message != "" {
            println!("{}", resp.message);
        }
        if let Some(data) = resp.data {
            println!("Details: {:#}", data);
        }
    } else {
        eprintln!("Error: {}", resp.message);
        std::process::exit(1);
    }
    Ok(())
}

async fn handle_command(cmd: Commands) -> anyhow::Result<()> {
    let socket_path = get_default_socket_path();

    match cmd {
        Commands::Ping => {
            exec_alme_cmd(&socket_path, || AlmeRequest { 
                command: AlmeCommand::Ping 
            }, "").await?;
        }

        Commands::LogTail { n } => {
            let req = AlmeRequest {
                command: AlmeCommand::LogTail { n }
            };
            let resp = send_alme_request(&socket_path, req).await?;
            if resp.success {
                if let Some(data) = resp.data {
                    if let Some(lines) = data.get("lines").and_then(|v| v.as_array()) {
                        for line in lines {
                            if let Some(s) = line.as_str() {
                                println!("{}", s);
                            }
                        }
                    }
                }
            } else {
                eprintln!("Error: {}", resp.message);
                std::process::exit(1);
            }
        }

        Commands::Status => {
            exec_alme_cmd(&socket_path, || AlmeRequest {
                command: AlmeCommand::Status { deployment_id: None }
            }, "").await?;
        }

        Commands::ModuleList => {
            exec_alme_cmd(&socket_path, || AlmeRequest {
                command: AlmeCommand::ModuleList
            }, "").await?;
        }

        Commands::ModuleInstall { path } => {
            let path = path.to_string_lossy().into_owned();
            exec_alme_cmd(&socket_path, || AlmeRequest {
                command: AlmeCommand::ModuleInstall { path: path.clone() }
            }, "").await?;
        }

        Commands::ModuleDeploy { file } => {
            let file = file.to_string_lossy().into_owned();
            exec_alme_cmd(&socket_path, || AlmeRequest {
                command: AlmeCommand::ModuleDeploy { file: file.clone() }
            }, "").await?;
        }

        Commands::ModuleStart { deployment_id } => {
            exec_alme_cmd(&socket_path, || AlmeRequest {
                command: AlmeCommand::ModuleStart { deployment_id: deployment_id.clone() }
            }, "").await?;
        }

        Commands::ModuleStop { deployment_id } => {
            exec_alme_cmd(&socket_path, || AlmeRequest {
                command: AlmeCommand::ModuleStop { deployment_id: deployment_id.clone() }
            }, "").await?;
        }

        Commands::Shell => {
            eprintln!("Interactive shell not implemented yet (use single commands)");
            std::process::exit(1);
        },
        //_ => {}
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse(); 
    handle_command(cli.command).await
}
