//! Cache management commands.

use anyhow::Result;
use clap::{Args, Subcommand};
use eurostat::{Config, EurostatClient};

/// Cache subcommands.
#[derive(Debug, Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommand,
}

/// Cache operations.
#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// Refresh catalogue cache from Eurostat.
    Refresh,
}

/// Execute cache command.
pub async fn run(args: CacheArgs, config: Config) -> Result<()> {
    match args.command {
        CacheCommand::Refresh => refresh(config).await,
    }
}

async fn refresh(config: Config) -> Result<()> {
    let client = EurostatClient::with_cache(config).await?;
    let datasets = client.catalogue().list_datasets().await?;
    if let Some(cache) = client.cache() {
        cache.upsert_datasets(&datasets).await?;
    }
    println!("Cached {} datasets", datasets.len());
    Ok(())
}
