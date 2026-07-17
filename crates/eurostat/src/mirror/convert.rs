//! Bulk file conversion to parquet.zstd bytes.

use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;

use crate::api::export::{export_columns_parquet_zstd_bytes, export_parquet_zstd_bytes};
use crate::client::EurostatClient;
use crate::error::{Error, Result};
use crate::model::BulkFile;
use crate::parse::comext_csv::parse_comext_csv;
use crate::parse::sdmx_csv::parse_sdmx_csv_table;
use crate::config::sanitize_dataset_id;
use crate::parse::tsv::parse_tsv;

/// Fetch and convert a bulk file into ZSTD parquet bytes (no local dataset dir required).
pub async fn convert_bulk_file_to_parquet(
    client: &EurostatClient,
    file: &BulkFile,
    source: &str,
) -> Result<(Vec<u8>, usize)> {
    let raw_bytes = client.http().get_bytes(&file.url).await?;
    if source == "comext" {
        let bytes = maybe_decompress(&raw_bytes)?;
        let csv_bytes = tokio::task::spawn_blocking(move || extract_csv_from_7z(&bytes))
            .await
            .map_err(|e| Error::Parse(format!("7z extraction task failed: {e}")))??;
        let table = parse_comext_csv(&csv_bytes)?;
        let row_count = table.rows.len();
        let parquet = export_columns_parquet_zstd_bytes(&table.headers, &table.rows)?;
        Ok((parquet, row_count))
    } else if file.format == "sdmx-csv" {
        let bytes = maybe_decompress(&raw_bytes)?;
        let table = parse_sdmx_csv_table(&bytes)?;
        let row_count = table.rows.len();
        let parquet = export_columns_parquet_zstd_bytes(&table.headers, &table.rows)?;
        Ok((parquet, row_count))
    } else {
        let raw = maybe_decompress(&raw_bytes)?;
        let table = parse_tsv(&raw, &file.dataset_id)?;
        let row_count = table.observations.len();
        let parquet = export_parquet_zstd_bytes(&table)?;
        Ok((parquet, row_count))
    }
}

/// Dataset family prefix used for object storage layout (`nama` from `nama_10_gdp`).
pub fn dataset_prefix(dataset_id: &str) -> String {
    let safe_id = sanitize_dataset_id(dataset_id);
    safe_id
        .split('_')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("other")
        .to_ascii_lowercase()
}

/// S3 object key for a mirrored dataset parquet file.
pub fn s3_object_key(dataset_id: &str, source: &str) -> String {
    let safe_id = sanitize_dataset_id(dataset_id);
    let filename = format!("{safe_id}.parquet.zstd");
    if source == "comext" {
        format!("datasets/comext/{filename}")
    } else {
        format!("datasets/{}/{filename}", dataset_prefix(dataset_id))
    }
}

pub(crate) fn output_path(
    config: &crate::config::Config,
    file: &BulkFile,
    source: &str,
) -> PathBuf {
    let stem = archive_stem(file);
    if source == "comext" {
        config
            .datasets_comext_dir()
            .unwrap_or_else(|_| PathBuf::from("datasets/comext"))
            .join(format!("{stem}.parquet.zstd"))
    } else {
        config
            .datasets_dir()
            .unwrap_or_else(|_| PathBuf::from("datasets"))
            .join(format!("{stem}.parquet.zstd"))
    }
}

fn archive_stem(file: &BulkFile) -> String {
    let base = file.url.split('?').next().unwrap_or(&file.url);
    let segment = base
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(&file.dataset_id);
    let stem = segment
        .trim_end_matches(".tsv.gz")
        .trim_end_matches(".7z")
        .trim_end_matches(".csv.gz");
    if stem.is_empty() || stem.contains('=') {
        file.dataset_id.clone()
    } else {
        stem.to_string()
    }
}

fn maybe_decompress(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        decompress_gzip(bytes)
    } else {
        Ok(bytes.to_vec())
    }
}

fn decompress_gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(bytes);
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut out)?;
    Ok(out)
}

fn extract_csv_from_7z(bytes: &[u8]) -> Result<Vec<u8>> {
    let tmp = tempfile::tempdir()?;
    let archive = tmp.path().join("input.7z");
    std::fs::write(&archive, bytes)?;
    let out_dir = tmp.path().join("out");
    std::fs::create_dir_all(&out_dir)?;

    let seven_zip = which_7z()?;
    let status = Command::new(&seven_zip)
        .args([
            "x",
            "-y",
            &format!("-o{}", out_dir.display()),
            &archive.to_string_lossy(),
        ])
        .status()
        .map_err(|e| Error::Parse(format!("failed to run {seven_zip}: {e}")))?;

    if !status.success() {
        return Err(Error::Parse(format!("{seven_zip} extraction failed")));
    }

    let csv_path = find_first_csv(&out_dir)?;
    Ok(std::fs::read(csv_path)?)
}

fn which_7z() -> Result<String> {
    for candidate in ["7z", "7za", "7zr"] {
        if Command::new(candidate)
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(candidate.to_string());
        }
    }
    Err(Error::Parse(
        "7z not found: install p7zip-full (apt install p7zip-full)".into(),
    ))
}

fn find_first_csv(dir: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "csv") {
            return Ok(path);
        }
        if path.is_dir() {
            if let Ok(found) = find_first_csv(&path) {
                return Ok(found);
            }
        }
    }
    Err(Error::Parse("no CSV file found inside 7z archive".into()))
}
