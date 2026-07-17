//! Dataset dimensions command.

use anyhow::Result;
use clap::Args;
use eurostat::sdmx::SdmxVersion;
use eurostat::{Config, EurostatClient};

/// List dimensions for a dataset.
#[derive(Debug, Args)]
pub struct DimensionsArgs {
    /// Dataset code.
    pub dataset_id: String,
    /// SDMX version to query.
    #[arg(long, default_value = "2.1")]
    pub version: String,
}

/// Execute dimensions lookup.
pub async fn run(args: DimensionsArgs, config: Config) -> Result<()> {
    let client = EurostatClient::new(config)?;
    let version = match args.version.as_str() {
        "3.0" | "3" => SdmxVersion::V3_0,
        _ => SdmxVersion::V2_1,
    };
    let dimensions = client.sdmx().dimensions(&args.dataset_id, version).await?;
    for dim in dimensions {
        println!("{} ({} codes)", dim.id, dim.codes.len());
        if let Some(label) = dim.label {
            println!("  label: {label}");
        }
    }
    Ok(())
}
