//! Local disk mirror pipeline.

use std::collections::HashSet;
use std::path::Path;

use futures::stream::{self, StreamExt};
use tracing::{info, warn};

use crate::client::EurostatClient;
use crate::config::Config;
use crate::model::BulkFile;

use super::convert::{convert_bulk_file_to_parquet, output_path};
use super::manifest::{append_manifest, load_completed_keys, manifest_key, ManifestEntry};
use super::{is_comext_archive, select_files, MirrorOptions, MirrorStatus};

/// Run the mirror pipeline to local disk.
pub async fn run_mirror(
    client: &EurostatClient,
    config: &Config,
    options: MirrorOptions,
) -> crate::error::Result<MirrorStatus> {
    config.ensure_data_dirs()?;
    let files = client.bulk().list_files().await?;
    let selected = select_files(&files, options.source);
    let limited: Vec<BulkFile> = match options.limit {
        Some(n) => selected.into_iter().take(n).collect(),
        None => selected,
    };

    let completed = if options.resume {
        load_completed_keys(config.manifest_path()?)?
    } else {
        HashSet::new()
    };

    let config = config.clone();
    let client = client.clone();
    let results = stream::iter(limited.into_iter())
        .map(|file| {
            let client = client.clone();
            let config = config.clone();
            let completed = completed.clone();
            async move { convert_file(&client, &config, &file, &completed).await }
        })
        .buffer_unordered(options.parallel.max(1))
        .collect::<Vec<_>>()
        .await;

    let mut status = MirrorStatus {
        total: results.len(),
        ..Default::default()
    };
    for entry in results {
        match entry.status.as_str() {
            "ok" => status.done += 1,
            "skipped" => status.skipped += 1,
            "failed" => status.failed += 1,
            _ => status.pending += 1,
        }
    }
    info!(
        total = status.total,
        done = status.done,
        failed = status.failed,
        skipped = status.skipped,
        "mirror run finished"
    );
    Ok(status)
}

async fn convert_file(
    client: &EurostatClient,
    config: &Config,
    file: &BulkFile,
    completed: &HashSet<String>,
) -> ManifestEntry {
    let source = if is_comext_archive(file) {
        "comext"
    } else {
        "dissemination"
    };
    let key = manifest_key(source, &file.dataset_id, &file.url);
    let output = output_path(config, file, source);

    if completed.contains(&key)
        || (output.exists() && output.metadata().map(|m| m.len() > 0).unwrap_or(false))
    {
        return ManifestEntry {
            source: source.to_string(),
            dataset_id: file.dataset_id.clone(),
            url: file.url.clone(),
            status: "skipped".to_string(),
            path: Some(output.display().to_string()),
            rows: None,
            error: None,
        };
    }

    match convert_file_inner(client, file, &output, source).await {
        Ok(rows) => {
            let entry = ManifestEntry {
                source: source.to_string(),
                dataset_id: file.dataset_id.clone(),
                url: file.url.clone(),
                status: "ok".to_string(),
                path: Some(output.display().to_string()),
                rows: Some(rows),
                error: None,
            };
            if let Err(err) = append_manifest(config, &entry) {
                warn!(%err, "failed to append manifest");
            }
            entry
        }
        Err(err) => {
            let entry = ManifestEntry {
                source: source.to_string(),
                dataset_id: file.dataset_id.clone(),
                url: file.url.clone(),
                status: "failed".to_string(),
                path: None,
                rows: None,
                error: Some(err.to_string()),
            };
            let _ = append_manifest(config, &entry);
            entry
        }
    }
}

async fn convert_file_inner(
    client: &EurostatClient,
    file: &BulkFile,
    output: &Path,
    source: &str,
) -> crate::error::Result<usize> {
    let (bytes, rows) = convert_bulk_file_to_parquet(client, file, source).await?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, bytes)?;
    Ok(rows)
}
