// arcella/arcella-cli/src/main.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};


/// Arcella Broker — микроброкер Arcella
#[derive(Parser)]
#[command(version, about = "Arcella Broker — микроброкер Arcella", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {

    /// Получить статус broker'а
    Status,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let _: Cli = Cli::parse(); 
    //handle_command(cli.command).await
    Ok(())
}