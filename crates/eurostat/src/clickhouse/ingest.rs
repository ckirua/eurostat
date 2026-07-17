//! Ingest orchestration — S3 parquet to ClickHouse.

use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use tracing::{info, warn};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::mirror::{s3_object_key, ManifestEntry, S3Config, S3_MANIFEST_KEY};

use super::catalogue::upsert_catalogue_metadata;
use super::client::ClickHouseClient;
use super::config::ClickHouseConfig;
use super::parquet::fetch_s3_parquet_dataset;

/// Default pilot / hero datasets for smoke testing.
pub const FEATURED_DATASETS: &[&str] = &[
    "nama_10_gdp",
    "prc_hicp_manr",
    "une_rt_m",
    "demo_pjan",
    "bop_c6_q",
];

/// Dataset candidate from the S3 manifest.
#[derive(Debug, Clone)]
pub struct ManifestDataset {
    pub dataset_id: String,
    pub s3_key: String,
    pub row_count: u64,
}

/// Ingest options.
#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub resume: bool,
    pub parallel: usize,
    pub limit: Option<usize>,
    pub datasets: Option<Vec<String>>,
    /// When true and `datasets` is unset, ingest hero datasets only.
    pub featured: bool,
}

/// Summary of an ingest run.
#[derive(Debug, Clone, Default)]
pub struct IngestStatus {
    pub total: usize,
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Ingest datasets from S3 into ClickHouse.
pub async fn run_ingest(
    s3: &S3Config,
    ch: &ClickHouseConfig,
    config: &Config,
    options: IngestOptions,
) -> Result<IngestStatus> {
    let client = Arc::new(ClickHouseClient::new(ch)?);
    client.ping().await?;

    let manifest = load_s3_manifest(s3).await?;
    let targets = resolve_targets(&options, &manifest);
    let limited: Vec<ManifestDataset> = match options.limit {
        Some(n) => targets.into_iter().take(n).collect(),
        None => targets,
    };

    let mut status = IngestStatus {
        total: limited.len(),
        ..Default::default()
    };

    let s3 = Arc::new(s3.clone());
    let config = Arc::new(config.clone());
    let resume = options.resume;

    let results = stream::iter(limited)
        .map(|target| {
            let s3 = s3.clone();
            let client = client.clone();
            let config = config.clone();
            async move { ingest_one_dataset(&s3, &client, &config, &target, resume).await }
        })
        .buffer_unordered(options.parallel.max(1))
        .collect::<Vec<_>>()
        .await;

    for result in results {
        match result {
            Ok(IngestOutcome::Done) => status.done += 1,
            Ok(IngestOutcome::Skipped) => status.skipped += 1,
            Err((dataset_id, err)) => {
                status.failed += 1;
                warn!(dataset_id = %dataset_id, %err, "dataset ingest failed");
                if let Err(log_err) = client
                    .insert_ingest_log(
                        &dataset_id,
                        &s3_object_key(&dataset_id, "dissemination"),
                        0,
                        0,
                        "failed",
                        Some(&err.to_string()),
                    )
                    .await
                {
                    warn!(%log_err, "failed to write ingest_log");
                }
            }
        }
    }

    info!(
        total = status.total,
        done = status.done,
        skipped = status.skipped,
        failed = status.failed,
        "ClickHouse ingest finished"
    );
    Ok(status)
}

enum IngestOutcome {
    Done,
    Skipped,
}

async fn ingest_one_dataset(
    s3: &S3Config,
    client: &ClickHouseClient,
    config: &Config,
    target: &ManifestDataset,
    resume: bool,
) -> std::result::Result<IngestOutcome, (String, Error)> {
    let dataset_id = &target.dataset_id;
    let s3_key = &target.s3_key;

    if resume
        && client
            .is_ingested_with_row_count(dataset_id, target.row_count)
            .await
            .map_err(|e| (dataset_id.clone(), e))?
    {
        info!(dataset_id, %s3_key, rows = target.row_count, "skipping unchanged dataset");
        return Ok(IngestOutcome::Skipped);
    }

    let dataset = fetch_s3_parquet_dataset(s3, dataset_id, "dissemination")
        .await
        .map_err(|e| (dataset_id.clone(), e))?;
    let row_count = dataset.rows.len() as u64;

    if dataset.rows.is_empty() {
        return Err((
            dataset_id.clone(),
            Error::Parse(format!("dataset {dataset_id} has no rows")),
        ));
    }

    client
        .insert_observations(&dataset.rows)
        .await
        .map_err(|e| (dataset_id.clone(), e))?;
    upsert_catalogue_metadata(client, config, &dataset, row_count)
        .await
        .map_err(|e| (dataset_id.clone(), e))?;

    client
        .insert_ingest_log(dataset_id, s3_key, target.row_count, row_count, "ok", None)
        .await
        .map_err(|e| (dataset_id.clone(), e))?;

    info!(dataset_id, %s3_key, row_count, "ingested dataset into ClickHouse");
    Ok(IngestOutcome::Done)
}

async fn load_s3_manifest(s3: &S3Config) -> Result<Vec<ManifestDataset>> {
    let bucket = s3.bucket()?;
    let response = bucket
        .get_object(S3_MANIFEST_KEY)
        .await
        .map_err(|e| Error::S3(e.to_string()))?;
    if response.status_code() == 404 {
        return Ok(Vec::new());
    }
    if !(200..300).contains(&response.status_code()) {
        return Err(Error::S3(format!(
            "manifest GET returned HTTP {}",
            response.status_code()
        )));
    }
    let content = String::from_utf8(response.to_vec())
        .map_err(|e| Error::S3(format!("manifest is not valid UTF-8: {e}")))?;
    Ok(parse_manifest_ok_entries(&content))
}

fn parse_manifest_ok_entries(content: &str) -> Vec<ManifestDataset> {
    let mut latest: HashMap<String, ManifestEntry> = HashMap::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<ManifestEntry>(line) else {
            continue;
        };
        if entry.status == "ok" {
            latest.insert(entry.dataset_id.clone(), entry);
        }
    }

    let mut datasets: Vec<ManifestDataset> = latest
        .into_values()
        .filter_map(|entry| {
            let s3_key = entry
                .path
                .clone()
                .unwrap_or_else(|| s3_object_key(&entry.dataset_id, "dissemination"));
            Some(ManifestDataset {
                dataset_id: entry.dataset_id,
                s3_key,
                row_count: entry.rows.unwrap_or(0) as u64,
            })
        })
        .collect();
    datasets.sort_by(|a, b| a.dataset_id.cmp(&b.dataset_id));
    datasets
}

fn resolve_targets(options: &IngestOptions, manifest: &[ManifestDataset]) -> Vec<ManifestDataset> {
    if let Some(ids) = &options.datasets {
        return ids
            .iter()
            .map(|id| ManifestDataset {
                dataset_id: id.clone(),
                s3_key: s3_object_key(id, "dissemination"),
                row_count: 0,
            })
            .collect();
    }
    if options.featured {
        return FEATURED_DATASETS
            .iter()
            .map(|id| ManifestDataset {
                dataset_id: (*id).to_string(),
                s3_key: s3_object_key(id, "dissemination"),
                row_count: 0,
            })
            .collect();
    }
    manifest.to_vec()
}
