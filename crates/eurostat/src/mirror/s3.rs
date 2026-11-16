//! Stream Eurostat bulk downloads directly to S3-compatible object storage.

use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Utc};
use futures::stream::{self, StreamExt};
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::Region;
use tracing::{info, warn};

use crate::client::EurostatClient;
use crate::error::{Error, Result};
use crate::model::BulkFile;

use super::convert::{convert_bulk_file_to_parquet, s3_object_key};
use super::manifest::ManifestEntry;
use super::s3_manifest::S3ManifestWriter;
use super::{is_comext_archive, select_files, MirrorOptions, MirrorStatus};

/// S3-compatible storage configuration (Hetzner Object Storage, etc.).
#[derive(Clone)]
pub struct S3Config {
    pub bucket: String,
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("bucket", &self.bucket)
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .finish()
    }
}

/// First non-empty trimmed value among `keys`.
fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl S3Config {
    /// Load credentials from environment variables.
    ///
    /// Preferred (shared `~/.env` layout):
    /// `S3_EUROSTAT_BUCKET`, `S3_URL`, `S3_REGION`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`
    ///
    /// Legacy eurostat-specific aliases still work:
    /// `S3_EUROSTAT_ENDPOINT`, `S3_EUROSTAT_REGION`, `S3_EUROSTAT_ACCESS_KEY`,
    /// `S3_EUROSTAT_SECRET_KEY`
    pub fn from_env() -> Result<Self> {
        let bucket = env_first(&["S3_EUROSTAT_BUCKET"]).unwrap_or_else(|| "eurostat".to_string());
        let endpoint = env_first(&["S3_EUROSTAT_ENDPOINT", "S3_URL"])
            .ok_or_else(|| Error::Config("S3_URL (or S3_EUROSTAT_ENDPOINT) is not set".into()))?;
        let region =
            env_first(&["S3_EUROSTAT_REGION", "S3_REGION"]).unwrap_or_else(|| "fsn1".to_string());
        let access_key =
            env_first(&["S3_EUROSTAT_ACCESS_KEY", "S3_ACCESS_KEY"]).ok_or_else(|| {
                Error::Config("S3_ACCESS_KEY (or S3_EUROSTAT_ACCESS_KEY) is not set".into())
            })?;
        let secret_key =
            env_first(&["S3_EUROSTAT_SECRET_KEY", "S3_SECRET_KEY"]).ok_or_else(|| {
                Error::Config("S3_SECRET_KEY (or S3_EUROSTAT_SECRET_KEY) is not set".into())
            })?;
        Ok(Self {
            bucket,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            region,
            access_key,
            secret_key,
        })
    }

    pub(crate) fn bucket(&self) -> Result<Box<Bucket>> {
        let credentials = Credentials::new(
            Some(&self.access_key),
            Some(&self.secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| Error::S3(e.to_string()))?;

        let region = Region::Custom {
            region: self.region.clone(),
            endpoint: self.endpoint.clone(),
        };

        let mut bucket =
            Bucket::new(&self.bucket, region, credentials).map_err(|e| Error::S3(e.to_string()))?;
        bucket.set_path_style();
        Ok(bucket)
    }
}

/// Fetch, convert, and upload datasets directly to S3 (no local `datasets/` dir).
pub async fn run_s3_mirror(
    client: &EurostatClient,
    s3: &S3Config,
    options: MirrorOptions,
) -> Result<MirrorStatus> {
    let files = client.bulk().list_files().await?;
    let selected = select_files(&files, options.source);
    let limited: Vec<BulkFile> = match options.limit {
        Some(n) => selected.into_iter().take(n).collect(),
        None => selected,
    };

    let client = client.clone();
    let s3 = Arc::new(s3.clone());
    let manifest = Arc::new(S3ManifestWriter::new(s3.clone()));
    let results = stream::iter(limited)
        .map(|file| {
            let client = client.clone();
            let s3 = s3.clone();
            let manifest = manifest.clone();
            async move {
                let bucket = match s3.bucket() {
                    Ok(bucket) => bucket,
                    Err(err) => {
                        warn!(dataset_id = %file.dataset_id, %err, "S3 bucket init failed");
                        let entry = failed_entry(&file, &err.to_string());
                        manifest.record(entry.clone()).await;
                        return entry;
                    }
                };
                let entry = upload_file(&client, &bucket, &file, options.resume).await;
                manifest.record(entry.clone()).await;
                entry
            }
        })
        .buffer_unordered(options.parallel.max(1))
        .collect::<Vec<_>>()
        .await;

    if let Err(err) = manifest.flush().await {
        warn!(%err, "failed to flush final S3 manifest batch");
    }

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
        "S3 mirror finished"
    );
    Ok(status)
}

async fn upload_file(
    client: &EurostatClient,
    bucket: &Bucket,
    file: &BulkFile,
    resume: bool,
) -> ManifestEntry {
    let source = if is_comext_archive(file) {
        "comext"
    } else {
        "dissemination"
    };
    let key = s3_object_key(&file.dataset_id, source);
    let change_stamp = inventory_change_stamp(file);
    let base = |status: &str, rows: Option<usize>, error: Option<String>| ManifestEntry {
        source: source.to_string(),
        dataset_id: file.dataset_id.clone(),
        url: file.url.clone(),
        status: status.to_string(),
        path: Some(key.clone()),
        rows,
        error,
        last_data_change: change_stamp.clone(),
    };

    if resume {
        match bucket.head_object(&key).await {
            Ok((head, code)) if (200..300).contains(&code) => {
                if object_covers_inventory(&head.last_modified, file) {
                    info!(
                        dataset_id = %file.dataset_id,
                        %key,
                        inventory = change_stamp.as_deref().unwrap_or("-"),
                        s3_modified = head.last_modified.as_deref().unwrap_or("-"),
                        "skipping unchanged S3 object"
                    );
                    return base("skipped", None, None);
                }
                info!(
                    dataset_id = %file.dataset_id,
                    %key,
                    inventory = change_stamp.as_deref().unwrap_or("-"),
                    s3_modified = head.last_modified.as_deref().unwrap_or("-"),
                    "S3 object stale vs Eurostat inventory; refreshing"
                );
            }
            Ok((_, 404)) | Err(_) => {}
            Ok((_, code)) => {
                warn!(
                    dataset_id = %file.dataset_id,
                    %key,
                    code,
                    "unexpected HEAD status, re-uploading"
                );
            }
        }
    }

    match convert_bulk_file_to_parquet(client, file, source).await {
        Ok((bytes, rows)) => {
            match bucket
                .put_object_with_content_type(&key, &bytes, "application/vnd.apache.parquet")
                .await
            {
                Ok(response) if (200..300).contains(&response.status_code()) => {
                    info!(
                        dataset_id = %file.dataset_id,
                        %key,
                        rows,
                        code = response.status_code(),
                        "uploaded to S3"
                    );
                    base("ok", Some(rows), None)
                }
                Ok(response) => {
                    let message = format!("S3 upload failed with HTTP {}", response.status_code());
                    warn!(dataset_id = %file.dataset_id, %key, "{}", message);
                    base("failed", None, Some(message))
                }
                Err(err) => {
                    warn!(dataset_id = %file.dataset_id, %key, %err, "S3 upload error");
                    base("failed", None, Some(err.to_string()))
                }
            }
        }
        Err(err) if err.is_temporarily_unavailable() => {
            warn!(
                dataset_id = %file.dataset_id,
                %key,
                %err,
                "dataset temporarily unavailable; skipping"
            );
            base("skipped", None, Some(err.to_string()))
        }
        Err(err) => {
            warn!(dataset_id = %file.dataset_id, %key, %err, "conversion failed");
            base("failed", None, Some(err.to_string()))
        }
    }
}

fn failed_entry(file: &BulkFile, error: &str) -> ManifestEntry {
    let source = if is_comext_archive(file) {
        "comext"
    } else {
        "dissemination"
    };
    ManifestEntry {
        source: source.to_string(),
        dataset_id: file.dataset_id.clone(),
        url: file.url.clone(),
        status: "failed".into(),
        path: None,
        rows: None,
        error: Some(error.to_string()),
        last_data_change: inventory_change_stamp(file),
    }
}

/// Best-effort stamp for manifest logging (later of data/structural change).
fn inventory_change_stamp(file: &BulkFile) -> Option<String> {
    match (
        file.last_data_change
            .as_deref()
            .and_then(parse_inventory_time),
        file.last_structural_change
            .as_deref()
            .and_then(parse_inventory_time),
        &file.last_data_change,
        &file.last_structural_change,
    ) {
        (Some(data), Some(structural), Some(data_s), Some(structural_s)) => {
            if data >= structural {
                Some(data_s.clone())
            } else {
                Some(structural_s.clone())
            }
        }
        (Some(_), None, Some(data_s), _) => Some(data_s.clone()),
        (None, Some(_), _, Some(structural_s)) => Some(structural_s.clone()),
        (_, _, Some(data_s), _) => Some(data_s.clone()),
        (_, _, _, Some(structural_s)) => Some(structural_s.clone()),
        _ => None,
    }
}

/// Skip when the S3 object is at least as new as the Eurostat inventory change.
///
/// If inventory timestamps are missing, keep the old skip-if-exists behavior.
fn object_covers_inventory(s3_last_modified: &Option<String>, file: &BulkFile) -> bool {
    let Some(inventory_ts) = latest_inventory_change(file) else {
        return true;
    };
    let Some(object_ts) = s3_last_modified
        .as_deref()
        .and_then(parse_s3_time)
        .map(|ts| ts.with_timezone(&Utc))
    else {
        // Object exists but mtime unreadable → refresh to be safe.
        return false;
    };
    object_ts >= inventory_ts
}

fn latest_inventory_change(file: &BulkFile) -> Option<DateTime<Utc>> {
    [
        file.last_data_change.as_deref(),
        file.last_structural_change.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(parse_inventory_time)
    .map(|ts| ts.with_timezone(&Utc))
    .max()
}

fn parse_inventory_time(raw: &str) -> Option<DateTime<FixedOffset>> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "-" {
        return None;
    }
    DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z")
        .or_else(|_| DateTime::parse_from_rfc3339(raw))
        .ok()
}

fn parse_s3_time(raw: &str) -> Option<DateTime<FixedOffset>> {
    let raw = raw.trim();
    // rust-s3 typically returns HTTP-date / RFC 2822 (`Mon, 16 Nov 2026 13:14:08 GMT`).
    DateTime::parse_from_rfc2822(raw)
        .or_else(|_| DateTime::parse_from_rfc3339(raw))
        .or_else(|_| DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z"))
        .or_else(|_| DateTime::parse_from_str(raw, "%a, %d %b %Y %H:%M:%S GMT"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mirror::{dataset_prefix, s3_object_key};

    #[test]
    fn s3_keys_use_dataset_prefix_folders() {
        assert_eq!(
            s3_object_key("nama_10_gdp", "dissemination"),
            "datasets/nama/nama_10_gdp.parquet.zstd"
        );
        assert_eq!(
            s3_object_key("ei_bssi_m_r2", "dissemination"),
            "datasets/ei/ei_bssi_m_r2.parquet.zstd"
        );
        assert_eq!(dataset_prefix("demo_pjan"), "demo");
    }

    fn sample_file(data: Option<&str>, structural: Option<&str>) -> BulkFile {
        BulkFile {
            dataset_id: "AACT_ALI01".into(),
            url: "https://example.com/?format=TSV".into(),
            format: "tsv".into(),
            size_bytes: None,
            last_data_change: data.map(str::to_string),
            last_structural_change: structural.map(str::to_string),
        }
    }

    #[test]
    fn skips_when_s3_mtime_covers_inventory_change() {
        let file = sample_file(
            Some("2026-11-11T11:00:00+0100"),
            Some("2026-03-24T11:00:00+0100"),
        );
        assert!(object_covers_inventory(
            &Some("2026-11-15T02:00:00+00:00".into()),
            &file
        ));
        // rust-s3 HEAD uses HTTP-date / RFC 2822.
        assert!(object_covers_inventory(
            &Some("Mon, 16 Nov 2026 13:14:08 GMT".into()),
            &file
        ));
    }

    #[test]
    fn refreshes_when_inventory_newer_than_s3_object() {
        let file = sample_file(
            Some("2026-11-11T11:00:00+0100"),
            Some("2026-03-24T11:00:00+0100"),
        );
        // Object uploaded in July; inventory says Nov data change → must refresh.
        assert!(!object_covers_inventory(
            &Some("2026-07-13T00:26:19+00:00".into()),
            &file
        ));
        assert!(!object_covers_inventory(
            &Some("Sat, 11 Jul 2026 12:06:27 GMT".into()),
            &file
        ));
    }

    #[test]
    fn without_inventory_timestamps_keeps_skip_if_exists() {
        let file = sample_file(None, None);
        assert!(object_covers_inventory(
            &Some("2026-07-13T00:26:19+00:00".into()),
            &file
        ));
    }

    #[test]
    fn parses_s3_http_date() {
        let ts = parse_s3_time("Mon, 16 Nov 2026 13:14:08 GMT").expect("http-date");
        assert_eq!(
            ts.with_timezone(&Utc).to_rfc3339(),
            "2026-11-16T13:14:08+00:00"
        );
    }
}
