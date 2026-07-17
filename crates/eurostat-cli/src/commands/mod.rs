//! CLI command definitions and dispatch.

mod bulk;
mod cache;
mod clickhouse;
mod completions;
mod dimensions;
mod fetch;
mod info;
mod mirror;
mod search;

use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use eurostat::Config;

/// Eurostat CLI — fetch and explore Eurostat datasets.
#[derive(Debug, Parser)]
#[command(name = "eurostat", version, about, long_about = None)]
pub struct Cli {
    /// Enable verbose logging.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Search cached datasets.
    Search(search::SearchArgs),
    /// Show dataset metadata.
    Info(info::InfoArgs),
    /// List dataset dimensions.
    Dimensions(dimensions::DimensionsArgs),
    /// Fetch dataset observations.
    Fetch(fetch::FetchArgs),
    /// Manage local catalogue cache.
    Cache(cache::CacheArgs),
    /// Bulk download operations.
    Bulk(bulk::BulkArgs),
    /// Mirror bulk files to parquet.zstd datasets.
    Mirror(mirror::MirrorArgs),
    /// Load S3 data into ClickHouse.
    Clickhouse(clickhouse::ClickhouseArgs),
    /// Generate shell completions.
    Completions {
        /// Target shell.
        shell: Shell,
    },
}

/// Run a parsed CLI command.
pub async fn run(command: Commands, config: Config) -> Result<()> {
    match command {
        Commands::Search(args) => search::run(args, config).await,
        Commands::Info(args) => info::run(args, config).await,
        Commands::Dimensions(args) => dimensions::run(args, config).await,
        Commands::Fetch(args) => fetch::run(args, config).await,
        Commands::Cache(args) => cache::run(args, config).await,
        Commands::Bulk(args) => bulk::run(args, config).await,
        Commands::Mirror(args) => mirror::run(args, config).await,
        Commands::Clickhouse(args) => clickhouse::run(args).await,
        Commands::Completions { .. } => Ok(()),
    }
}
