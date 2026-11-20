//! Mirror progress manifest (JSONL).

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::Config;
use crate::error::Result;

use super::MirrorStatus;

/// One manifest entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub source: String,
    pub dataset_id: String,
    pub url: String,
    pub status: String,
    pub path: Option<String>,
    pub rows: Option<usize>,
    pub error: Option<String>,
    /// Eurostat inventory change stamp mirrored into this object (when known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_data_change: Option<String>,
}

/// Summarize manifest state.
pub fn mirror_status(config: &Config) -> Result<MirrorStatus> {
    let path = config.manifest_path()?;
    if !path.exists() {
        return Ok(MirrorStatus::default());
    }
    let reader = BufReader::new(File::open(path)?);
    mirror_status_from_reader(reader)
}

/// Summarize mirror progress from JSONL text (local or S3 manifest body).
pub fn mirror_status_from_str(content: &str) -> Result<MirrorStatus> {
    mirror_status_from_reader(content.as_bytes())
}

fn mirror_status_from_reader<R: BufRead>(reader: R) -> Result<MirrorStatus> {
    let mut status = MirrorStatus::default();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry = match serde_json::from_str::<ManifestEntry>(&line) {
            Ok(entry) => entry,
            // Tolerate foreign or corrupted lines so a shared manifest can't
            // block status reporting (e.g. other apps appending to the same
            // S3 object).
            Err(err) => {
                warn!(line = index + 1, error = %err, "skipping invalid manifest line");
                continue;
            }
        };
        apply_entry_to_status(&mut status, &entry);
    }
    Ok(status)
}

fn apply_entry_to_status(status: &mut MirrorStatus, entry: &ManifestEntry) {
    status.total += 1;
    match entry.status.as_str() {
        "ok" => status.done += 1,
        "skipped" => status.skipped += 1,
        "failed" => status.failed += 1,
        _ => status.pending += 1,
    }
}

/// Serialize manifest entries as JSONL bytes.
pub(crate) fn entries_to_jsonl(entries: &[ManifestEntry]) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    for entry in entries {
        serde_json::to_writer(&mut body, entry)?;
        body.push(b'\n');
    }
    Ok(body)
}

pub(crate) fn manifest_key(source: &str, dataset_id: &str, url: &str) -> String {
    format!("{source}:{dataset_id}:{url}")
}

pub(crate) fn load_completed_keys(path: std::path::PathBuf) -> Result<HashSet<String>> {
    let mut keys = HashSet::new();
    if !path.exists() {
        return Ok(keys);
    }
    let reader = BufReader::new(File::open(path)?);
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry = match serde_json::from_str::<ManifestEntry>(&line) {
            Ok(entry) => entry,
            Err(err) => {
                warn!(line = index + 1, error = %err, "skipping invalid manifest line");
                continue;
            }
        };
        if entry.status == "ok" || entry.status == "skipped" {
            keys.insert(manifest_key(&entry.source, &entry.dataset_id, &entry.url));
        }
    }
    Ok(keys)
}

pub(crate) fn append_manifest(config: &Config, entry: &ManifestEntry) -> Result<()> {
    let path = config.manifest_path()?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, entry)?;
    file.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_skips_foreign_lines_and_counts_valid_entries() {
        let manifest = [
            r#"{"source":"dissemination","dataset_id":"AACT_ALI01","url":"https://x/a","status":"ok","path":null,"rows":10,"error":null}"#,
            // Live repro: a record from another app sharing the object, with no
            // `source` field.
            r#"{"market":"um","frequency":"daily","data_type":"aggTrades","symbol":"BTCUSDT","period":"2024-01-15","interval":null,"source_url":"https://data.binance.vision/...","status":"ok"}"#,
            r#"not json at all"#,
            r#"{"source":"dissemination","dataset_id":"AACT_ALI02","url":"https://x/b","status":"failed","path":null,"rows":null,"error":"boom"}"#,
            r#"{"source":"dissemination","dataset_id":"AACT_ALI03","url":"https://x/c","status":"skipped","path":null,"rows":null,"error":null}"#,
        ]
        .join("\n");

        let status = mirror_status_from_str(&manifest).expect("foreign lines must not be fatal");
        assert_eq!(status.total, 3);
        assert_eq!(status.done, 1);
        assert_eq!(status.failed, 1);
        assert_eq!(status.skipped, 1);
        assert_eq!(status.pending, 0);
    }
}
