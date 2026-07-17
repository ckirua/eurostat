//! Dataset info command.

use anyhow::Result;
use clap::Args;
use eurostat::{Config, EurostatClient};

/// Show dataset metadata.
#[derive(Debug, Args)]
pub struct InfoArgs {
    /// Dataset code.
    pub dataset_id: String,
}

/// Execute dataset info lookup.
pub async fn run(args: InfoArgs, config: Config) -> Result<()> {
    let client = EurostatClient::with_cache(config).await?;
    if let Some(cache) = client.cache() {
        if let Some(dataset) = cache.get_dataset(&args.dataset_id).await? {
            print_dataset(&dataset);
            return Ok(());
        }
    }

    let dataset = client.catalogue().dataset_info(&args.dataset_id).await?;
    print_dataset(&dataset);
    Ok(())
}

fn print_dataset(dataset: &eurostat::model::DatasetInfo) {
    println!("id: {}", dataset.id);
    println!("title: {}", dataset.title);
    if let Some(desc) = &dataset.description {
        println!("description: {desc}");
    }
    if let Some(updated) = &dataset.updated_at {
        println!("updated: {updated}");
    }
    if let Some(url) = &dataset.url {
        println!("url: {url}");
    }
}
