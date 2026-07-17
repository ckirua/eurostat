//! Mirror progress manifest (JSONL).

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};

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
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: ManifestEntry = serde_json::from_str(&line)?;
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
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: ManifestEntry = serde_json::from_str(&line)?;
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
