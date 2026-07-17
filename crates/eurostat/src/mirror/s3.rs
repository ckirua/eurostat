//! Stream Eurostat bulk downloads directly to S3-compatible object storage.

use std::sync::Arc;

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

impl S3Config {
    /// Load credentials from environment variables.
    ///
    /// Required: `S3_EUROSTAT_ACCESS_KEY`, `S3_EUROSTAT_SECRET_KEY`, `S3_EUROSTAT_ENDPOINT`
    /// Optional: `S3_EUROSTAT_BUCKET` (default `eurostat`), `S3_EUROSTAT_REGION` (default `fsn1`)
    pub fn from_env() -> Result<Self> {
        let bucket = std::env::var("S3_EUROSTAT_BUCKET")
            .unwrap_or_else(|_| "eurostat".to_string())
            .trim()
            .to_string();
        let endpoint = std::env::var("S3_EUROSTAT_ENDPOINT")
            .map_err(|_| Error::Config("S3_EUROSTAT_ENDPOINT is not set".into()))?
            .trim()
            .trim_end_matches('/')
            .to_string();
        let region = std::env::var("S3_EUROSTAT_REGION")
            .unwrap_or_else(|_| "fsn1".to_string())
            .trim()
            .to_string();
        let access_key = std::env::var("S3_EUROSTAT_ACCESS_KEY")
            .map_err(|_| Error::Config("S3_EUROSTAT_ACCESS_KEY is not set".into()))?
            .trim()
            .to_string();
        let secret_key = std::env::var("S3_EUROSTAT_SECRET_KEY")
            .map_err(|_| Error::Config("S3_EUROSTAT_SECRET_KEY is not set".into()))?
            .trim()
            .to_string();
        Ok(Self {
            bucket,
            endpoint,
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
    let results = stream::iter(limited.into_iter())
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
    let base = |status: &str, rows: Option<usize>, error: Option<String>| ManifestEntry {
        source: source.to_string(),
        dataset_id: file.dataset_id.clone(),
        url: file.url.clone(),
        status: status.to_string(),
        path: Some(key.clone()),
        rows,
        error,
    };

    if resume {
        match bucket.head_object(&key).await {
            Ok((_, code)) if (200..300).contains(&code) => {
                info!(dataset_id = %file.dataset_id, %key, code, "skipping existing S3 object");
                return base("skipped", None, None);
            }
            Ok((_, 404)) | Err(_) => {}
            Ok((_, code)) => {
                warn!(dataset_id = %file.dataset_id, %key, code, "unexpected HEAD status, re-uploading");
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
    }
}

#[cfg(test)]
mod tests {
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
}
