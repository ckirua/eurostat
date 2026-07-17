//! Dataset search command.

use anyhow::Result;
use clap::Args;
use eurostat::{Config, EurostatClient};

/// Search cached datasets by keyword.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Search query.
    pub query: String,
    /// Maximum number of results.
    #[arg(short, long, default_value_t = 20)]
    pub limit: usize,
}

/// Execute dataset search.
pub async fn run(args: SearchArgs, config: Config) -> Result<()> {
    let client = EurostatClient::with_cache(config).await?;
    let cache = client
        .cache()
        .ok_or_else(|| anyhow::anyhow!("cache feature is required for search"))?;
    let engine = eurostat::search::SearchEngine::new(cache);
    let results = engine.search(&args.query, args.limit).await?;
    if results.is_empty() {
        println!(
            "No datasets matched '{}'. Try `eurostat cache refresh` first.",
            args.query
        );
        return Ok(());
    }
    for dataset in results {
        println!("{}\t{}", dataset.id, dataset.title);
    }
    Ok(())
}
