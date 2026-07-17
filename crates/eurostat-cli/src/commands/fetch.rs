//! Dataset fetch command.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use eurostat::export::export_table;
use eurostat::model::{DataFormat, OutputFormat};
use eurostat::sdmx::SdmxVersion;
use eurostat::{Config, EurostatClient};

/// Fetch dataset observations.
#[derive(Debug, Args)]
pub struct FetchArgs {
    /// Dataset code.
    pub dataset_id: String,
    /// API to use: statistics, sdmx21, sdmx30.
    #[arg(long, default_value = "statistics")]
    pub api: String,
    /// Dimension filters as key=value pairs.
    #[arg(long = "filter", value_parser = parse_filter)]
    pub filters: Vec<(String, String)>,
    /// SDMX key (dot-separated dimension values).
    #[arg(long)]
    pub key: Option<String>,
    /// Response language.
    #[arg(long)]
    pub lang: Option<String>,
    /// Output format: csv, json, parquet.
    #[arg(long, default_value = "json")]
    pub format: String,
    /// Output file path (defaults to `~/.local/share/eurostat/exports/` for file formats).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Use async job API for large queries.
    #[arg(long)]
    pub r#async: bool,
}

fn parse_filter(raw: &str) -> Result<(String, String), String> {
    raw.split_once('=')
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .ok_or_else(|| "filter must be key=value".to_string())
}

/// Execute dataset fetch.
pub async fn run(args: FetchArgs, config: Config) -> Result<()> {
    let client = EurostatClient::new(config.clone())?;
    let filters: HashMap<String, String> = args.filters.into_iter().collect();
    let output_format: OutputFormat = args
        .format
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let table = match args.api.as_str() {
        "sdmx21" | "sdmx2.1" | "sdmx-2.1" => {
            client
                .sdmx()
                .fetch(
                    &args.dataset_id,
                    args.key.as_deref(),
                    &filters,
                    DataFormat::JsonStat,
                    SdmxVersion::V2_1,
                )
                .await?
        }
        "sdmx30" | "sdmx3.0" | "sdmx-3.0" => {
            client
                .sdmx()
                .fetch(
                    &args.dataset_id,
                    args.key.as_deref(),
                    &filters,
                    DataFormat::JsonStat,
                    SdmxVersion::V3_0,
                )
                .await?
        }
        _ => {
            client
                .statistics()
                .fetch(&args.dataset_id, &filters, args.lang.as_deref())
                .await?
        }
    };

    if output_format == OutputFormat::Json && args.output.is_none() {
        let json = serde_json::to_string_pretty(&table)?;
        println!("{json}");
        return Ok(());
    }

    let path = match args.output {
        Some(path) => path,
        None => {
            config.ensure_data_dirs()?;
            config.default_export_path(&args.dataset_id, output_format)?
        }
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    export_table(&table, output_format, &path).context("export failed")?;
    println!("Wrote {} observations to {}", table.len(), path.display());
    Ok(())
}
