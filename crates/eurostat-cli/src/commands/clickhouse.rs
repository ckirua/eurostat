//! ClickHouse subcommands.

use anyhow::Result;
use clap::{Args, Subcommand};
use eurostat::{
    load_codelists, run_ingest, ClickHouseConfig, Config, IngestOptions, IngestStatus,
    LoadCodelistsOptions, S3Config,
};
use std::path::PathBuf;

/// ClickHouse operations.
#[derive(Debug, Args)]
pub struct ClickhouseArgs {
    #[command(subcommand)]
    pub command: ClickhouseCommand,
}

/// ClickHouse subcommands.
#[derive(Debug, Subcommand)]
pub enum ClickhouseCommand {
    /// Ingest S3 parquet datasets into ClickHouse.
    Ingest(IngestArgs),
    /// Load codelists from public/codelists into ClickHouse.
    LoadCodelists(LoadCodelistsArgs),
    /// Show ingest progress from ingest_log.
    Status,
}

/// Load codelists arguments.
#[derive(Debug, Args)]
pub struct LoadCodelistsArgs {
    /// Directory containing catalogue.tsv and inventory.tsv.
    #[arg(long, default_value = "public/codelists")]
    pub public_dir: PathBuf,
    /// Comma-separated codelist ids (default: GEO,UNIT).
    #[arg(long, value_delimiter = ',')]
    pub codelists: Option<Vec<String>>,
    /// Load all codelists from inventory.tsv.
    #[arg(long)]
    pub all: bool,
}

/// Ingest arguments.
#[derive(Debug, Args)]
pub struct IngestArgs {
    /// Skip datasets already recorded in ingest_log.
    #[arg(long)]
    pub resume: bool,
    /// Parallel ingest workers (sequential per dataset for now).
    #[arg(long, default_value_t = 4)]
    pub parallel: usize,
    /// Limit number of datasets.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Ingest hero datasets only (default: all S3 manifest `ok` entries).
    #[arg(long)]
    pub featured: bool,
    /// Comma-separated dataset ids (overrides manifest).
    #[arg(long, value_delimiter = ',')]
    pub datasets: Option<Vec<String>>,
}

/// Execute clickhouse command.
pub async fn run(args: ClickhouseArgs) -> Result<()> {
    match args.command {
        ClickhouseCommand::Ingest(ingest) => {
            let s3 = S3Config::from_env().map_err(anyhow::Error::from)?;
            let ch = ClickHouseConfig::from_env().map_err(anyhow::Error::from)?;
            let config = Config::load()?;
            let status = run_ingest(
                &s3,
                &ch,
                &config,
                IngestOptions {
                    resume: ingest.resume,
                    parallel: ingest.parallel,
                    limit: ingest.limit,
                    datasets: ingest.datasets,
                    featured: ingest.featured,
                },
            )
            .await
            .map_err(anyhow::Error::from)?;
            print_status(&status);
            if status.failed > 0 {
                anyhow::bail!("ingest completed with {} failures", status.failed);
            }
            Ok(())
        }
        ClickhouseCommand::LoadCodelists(args) => {
            let ch = ClickHouseConfig::from_env().map_err(anyhow::Error::from)?;
            let client = eurostat::ClickHouseClient::new(&ch).map_err(anyhow::Error::from)?;
            let status = load_codelists(
                &client,
                LoadCodelistsOptions {
                    public_dir: args.public_dir,
                    codelists: args.codelists,
                    all: args.all,
                },
            )
            .await
            .map_err(anyhow::Error::from)?;
            println!(
                "codelists: loaded {} codelists, {} codes",
                status.codelists, status.codes
            );
            Ok(())
        }
        ClickhouseCommand::Status => {
            let ch = ClickHouseConfig::from_env().map_err(anyhow::Error::from)?;
            let status = fetch_status(&ch).await?;
            print_status(&status);
            Ok(())
        }
    }
}

fn print_status(status: &IngestStatus) {
    println!(
        "ingest: total={} done={} skipped={} failed={}",
        status.total, status.done, status.skipped, status.failed
    );
}

async fn fetch_status(ch: &ClickHouseConfig) -> Result<IngestStatus> {
    use clickhouse::Client;

    let client = Client::default()
        .with_url(ch.url())
        .with_user(&ch.user)
        .with_password(&ch.password)
        .with_database(&ch.database);

    #[derive(clickhouse::Row, serde::Deserialize)]
    struct CountRow {
        status: String,
        count: u64,
    }

    let rows: Vec<CountRow> = client
        .query(&format!(
            "SELECT toString(status) AS status, count() AS count FROM {}.ingest_log GROUP BY status",
            ch.database
        ))
        .fetch_all()
        .await
        .map_err(|e| anyhow::anyhow!("ClickHouse query failed: {e}"))?;

    let mut status = IngestStatus::default();
    for row in rows {
        status.total += row.count as usize;
        match row.status.as_str() {
            "ok" => status.done = row.count as usize,
            "skipped" => status.skipped = row.count as usize,
            "failed" => status.failed = row.count as usize,
            _ => {}
        }
    }
    Ok(status)
}
