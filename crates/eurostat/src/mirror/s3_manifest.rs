//! S3 manifest (`manifest/manifest.jsonl`) — batched append uploads.

use std::sync::Arc;

use s3::bucket::Bucket;
use tokio::sync::Mutex;
use tracing::warn;

use crate::error::{Error, Result};

use super::manifest::{entries_to_jsonl, mirror_status_from_str, ManifestEntry};
use super::{MirrorStatus, S3Config};

/// Object key for the mirror progress log in the bucket.
pub const S3_MANIFEST_KEY: &str = "manifest/manifest.jsonl";

const FLUSH_BATCH: usize = 32;

/// Buffers manifest entries and periodically flushes them to S3.
pub struct S3ManifestWriter {
    config: Arc<S3Config>,
    buffer: Mutex<Vec<ManifestEntry>>,
}

impl S3ManifestWriter {
    pub fn new(config: Arc<S3Config>) -> Self {
        Self {
            config,
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// Queue an entry; flush when the batch size is reached.
    pub async fn record(&self, entry: ManifestEntry) {
        let should_flush = {
            let mut buffer = self.buffer.lock().await;
            buffer.push(entry);
            buffer.len() >= FLUSH_BATCH
        };
        if should_flush {
            if let Err(err) = self.flush().await {
                warn!(%err, "failed to flush S3 manifest batch");
            }
        }
    }

    /// Upload any buffered entries (call at end of a mirror run).
    pub async fn flush(&self) -> Result<()> {
        let entries = {
            let mut buffer = self.buffer.lock().await;
            if buffer.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *buffer)
        };

        let bucket = self.config.bucket()?;
        let existing = fetch_manifest_bytes(&bucket).await?;
        let mut body = existing;
        body.extend(entries_to_jsonl(&entries)?);

        put_manifest(&bucket, &body).await
    }
}

/// Read mirror progress from the S3 manifest object.
pub async fn mirror_status_s3(s3: &S3Config) -> Result<MirrorStatus> {
    let bucket = s3.bucket()?;
    let bytes = fetch_manifest_bytes(&bucket).await?;
    if bytes.is_empty() {
        return Ok(MirrorStatus::default());
    }
    let content = String::from_utf8(bytes)
        .map_err(|e| Error::S3(format!("manifest is not valid UTF-8: {e}")))?;
    mirror_status_from_str(&content)
}

async fn fetch_manifest_bytes(bucket: &Bucket) -> Result<Vec<u8>> {
    match bucket.get_object(S3_MANIFEST_KEY).await {
        Ok(response) if (200..300).contains(&response.status_code()) => Ok(response.to_vec()),
        Ok(response) if response.status_code() == 404 => Ok(Vec::new()),
        Ok(response) => Err(Error::S3(format!(
            "manifest GET returned HTTP {}",
            response.status_code()
        ))),
        Err(err) => {
            let message = err.to_string();
            if message.contains("404") || message.contains("Not Found") {
                Ok(Vec::new())
            } else {
                Err(Error::S3(message))
            }
        }
    }
}

async fn put_manifest(bucket: &Bucket, body: &[u8]) -> Result<()> {
    let response = bucket
        .put_object_with_content_type(S3_MANIFEST_KEY, body, "application/x-ndjson")
        .await
        .map_err(|e| Error::S3(e.to_string()))?;
    if (200..300).contains(&response.status_code()) {
        Ok(())
    } else {
        Err(Error::S3(format!(
            "manifest PUT returned HTTP {}",
            response.status_code()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_key_is_under_manifest_prefix() {
        assert_eq!(S3_MANIFEST_KEY, "manifest/manifest.jsonl");
    }

    #[test]
    fn entries_to_jsonl_produces_one_line_per_entry() {
        let entries = vec![
            ManifestEntry {
                source: "dissemination".into(),
                dataset_id: "nama_10_gdp".into(),
                url: "https://example.com/nama.tsv.gz".into(),
                status: "ok".into(),
                path: Some("datasets/nama/nama_10_gdp.parquet.zstd".into()),
                rows: Some(100),
                error: None,
            },
            ManifestEntry {
                source: "dissemination".into(),
                dataset_id: "demo".into(),
                url: "https://example.com/demo.tsv.gz".into(),
                status: "failed".into(),
                path: None,
                rows: None,
                error: Some("parse error".into()),
            },
        ];
        let body = entries_to_jsonl(&entries).expect("serialize");
        let text = String::from_utf8(body).expect("utf8");
        assert_eq!(text.lines().count(), 2);
        let status = mirror_status_from_str(&text).expect("parse");
        assert_eq!(status.total, 2);
        assert_eq!(status.done, 1);
        assert_eq!(status.failed, 1);
    }
}
