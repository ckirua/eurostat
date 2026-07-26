//! Mirror subcommands.
use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use eurostat::{
    mirror_status, mirror_status_s3, run_mirror, run_s3_mirror, Config, EurostatClient,
    MirrorOptions, MirrorSource, S3Config,
};

/// Mirror subcommands.
#[derive(Debug, Args)]
pub struct MirrorArgs {
    #[command(subcommand)]
    pub command: MirrorCommand,
}

/// Mirror operations.
#[derive(Debug, Subcommand)]
pub enum MirrorCommand {
    /// Show mirror progress from manifest.jsonl (local or S3).
    Status(StatusArgs),
    /// Download and convert bulk files to parquet.zstd.
    Run(RunArgs),
    /// Fetch, convert, and upload directly to S3 (no local datasets dir).
    S3(S3RunArgs),
}

/// Mirror status arguments.
#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Read progress from `s3://{bucket}/manifest/manifest.jsonl` instead of local disk.
    #[arg(long)]
    pub s3: bool,
}

/// Bulk source filter.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SourceArg {
    Dissemination,
    Comext,
    All,
}

impl From<SourceArg> for MirrorSource {
    fn from(value: SourceArg) -> Self {
        match value {
            SourceArg::Dissemination => MirrorSource::Dissemination,
            SourceArg::Comext => MirrorSource::Comext,
            SourceArg::All => MirrorSource::All,
        }
    }
}

/// Mirror run arguments.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Bulk source to mirror.
    #[arg(long, value_enum, default_value_t = SourceArg::All)]
    pub source: SourceArg,
    /// Skip files already recorded in manifest or present on disk.
    #[arg(long)]
    pub resume: bool,
    /// Parallel conversions.
    #[arg(long, default_value_t = 4)]
    pub parallel: usize,
    /// Limit number of files (smoke testing).
    #[arg(long)]
    pub limit: Option<usize>,
}

/// S3 mirror arguments (reads `S3_EUROSTAT_BUCKET` + shared `S3_*` env vars).
#[derive(Debug, Args)]
pub struct S3RunArgs {
    /// Bulk source to mirror.
    #[arg(long, value_enum, default_value_t = SourceArg::Dissemination)]
    pub source: SourceArg,
    /// Skip objects that already exist in the bucket.
    #[arg(long)]
    pub resume: bool,
    /// Parallel fetch+upload workers.
    #[arg(long, default_value_t = 8)]
    pub parallel: usize,
    /// Limit number of files (smoke testing).
    #[arg(long)]
    pub limit: Option<usize>,
}

/// Execute mirror command.
pub async fn run(args: MirrorArgs, config: Config) -> Result<()> {
    match args.command {
        MirrorCommand::Status(args) => {
            let from_s3 = args.s3;
            let status = if from_s3 {
                let s3 = S3Config::from_env().map_err(anyhow::Error::from)?;
                mirror_status_s3(&s3).await.map_err(anyhow::Error::from)?
            } else {
                mirror_status(&config)?
            };
            let label = if from_s3 { "S3 mirror" } else { "mirror" };
            println!(
                "{label}: total={} done={} failed={} skipped={} pending={}",
                status.total, status.done, status.failed, status.skipped, status.pending
            );
            Ok(())
        }
        MirrorCommand::Run(run) => {
            let client = EurostatClient::new(config)?;
            let status = run_mirror(
                &client,
                client.config(),
                MirrorOptions {
                    source: run.source.into(),
                    resume: run.resume,
                    parallel: run.parallel,
                    limit: run.limit,
                },
            )
            .await?;
            println!(
                "mirror finished: total={} done={} failed={} skipped={} pending={}",
                status.total, status.done, status.failed, status.skipped, status.pending
            );
            if status.failed > 0 {
                anyhow::bail!("mirror completed with {} failures", status.failed);
            }
            Ok(())
        }
        MirrorCommand::S3(run) => {
            let s3 = S3Config::from_env().map_err(anyhow::Error::from)?;
            let client = EurostatClient::new(config)?;
            let status = run_s3_mirror(
                &client,
                &s3,
                MirrorOptions {
                    source: run.source.into(),
                    resume: run.resume,
                    parallel: run.parallel,
                    limit: run.limit,
                },
            )
            .await?;
            println!(
                "S3 mirror finished: total={} done={} failed={} skipped={} pending={}",
                status.total, status.done, status.failed, status.skipped, status.pending
            );
            if status.failed > 0 {
                anyhow::bail!("S3 mirror completed with {} failures", status.failed);
            }
            Ok(())
        }
    }
}
