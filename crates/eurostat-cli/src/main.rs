//! Eurostat CLI entry point.

mod commands;
mod logging;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use eurostat::Config;

use crate::commands::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(cli.verbose)?;

    match cli.command {
        commands::Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "eurostat", &mut std::io::stdout());
            Ok(())
        }
        command => {
            let config = Config::load()?;
            commands::run(command, config).await
        }
    }
}
