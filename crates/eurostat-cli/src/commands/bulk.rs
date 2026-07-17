//! Bulk download commands.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand};
use eurostat::{Config, EurostatClient};

/// Bulk download subcommands.
#[derive(Debug, Args)]
pub struct BulkArgs {
    #[command(subcommand)]
    pub command: BulkCommand,
}

/// Bulk operations.
#[derive(Debug, Subcommand)]
pub enum BulkCommand {
    /// List available bulk files.
    List,
    /// Download bulk files for a dataset.
    Download(DownloadArgs),
}

/// Bulk download arguments.
#[derive(Debug, Args)]
pub struct DownloadArgs {
    /// Dataset code prefix to match.
    pub dataset_id: String,
    /// Destination directory (defaults to `~/.local/share/eurostat/bulk`).
    #[arg(short, long)]
    pub dest: Option<PathBuf>,
    /// Skip files that already exist.
    #[arg(long)]
    pub resume: bool,
    /// Parallel download count.
    #[arg(long, default_value_t = 4)]
    pub parallel: usize,
}

/// Execute bulk command.
pub async fn run(args: BulkArgs, config: Config) -> Result<()> {
    let client = EurostatClient::new(config.clone())?;
    match args.command {
        BulkCommand::List => {
            let files = client.bulk().list_files().await?;
            for file in files.iter().take(50) {
                println!("{}\t{}\t{}", file.dataset_id, file.format, file.url);
            }
            if files.len() > 50 {
                println!("... and {} more", files.len() - 50);
            }
            Ok(())
        }
        BulkCommand::Download(download) => {
            let dest = match download.dest {
                Some(dest) => dest,
                None => {
                    config.ensure_data_dirs()?;
                    config.bulk_dir()?
                }
            };
            let files = client
                .bulk()
                .list_files()
                .await?
                .into_iter()
                .filter(|f| f.dataset_id.contains(&download.dataset_id))
                .collect::<Vec<_>>();
            if files.is_empty() {
                anyhow::bail!("no bulk files matched '{}'", download.dataset_id);
            }
            let paths = client
                .bulk()
                .download_many(&files, &dest, download.resume, download.parallel, true)
                .await?;
            println!("Downloaded {} files to {}", paths.len(), dest.display());
            Ok(())
        }
    }
}
