//! Load Eurostat codelists into ClickHouse `codelists` and `codes` tables.

use std::path::{Path, PathBuf};

use serde_json::json;
use tracing::info;

use crate::error::{Error, Result};

use super::client::ClickHouseClient;

/// Default codelists for label dictionaries.
pub const DEFAULT_CODELISTS: &[&str] = &["GEO", "UNIT"];

/// Options for codelist loading.
#[derive(Debug, Clone)]
pub struct LoadCodelistsOptions {
    pub public_dir: PathBuf,
    pub codelists: Option<Vec<String>>,
    pub all: bool,
}

/// Summary of a codelist load run.
#[derive(Debug, Clone, Default)]
pub struct LoadCodelistsStatus {
    pub codelists: usize,
    pub codes: usize,
}

/// Load codelists from `public/codelists/` into ClickHouse.
pub async fn load_codelists(
    client: &ClickHouseClient,
    options: LoadCodelistsOptions,
) -> Result<LoadCodelistsStatus> {
    let catalogue_path = options.public_dir.join("catalogue.tsv");
    let inventory_path = options.public_dir.join("inventory.tsv");

    let catalogue_entries = parse_catalogue_tsv(&catalogue_path)?;
    let inventory_entries = parse_inventory_tsv(&inventory_path)?;

    let targets = resolve_targets(&options, &inventory_entries);
    let mut status = LoadCodelistsStatus::default();

    let mut codelist_lines = Vec::new();
    for entry in &catalogue_entries {
        if !targets.contains(&entry.id) {
            continue;
        }
        codelist_lines.push(
            serde_json::to_string(&json!({
                "codelist_id": entry.id,
                "label": entry.label,
                "last_update": entry.last_update,
                "is_standard": entry.is_standard,
            }))
            .map_err(|e| Error::ClickHouse(e.to_string()))?,
        );
    }
    if !codelist_lines.is_empty() {
        client.insert_json_lines("codelists", &codelist_lines).await?;
        status.codelists = codelist_lines.len();
    }

    let http = reqwest::Client::new();
    for codelist_id in &targets {
        let Some(inv) = inventory_entries.iter().find(|e| &e.id == codelist_id) else {
            continue;
        };
        let bytes = http
            .get(&inv.latest_tsv_url)
            .send()
            .await
            .map_err(|e| Error::Http(e))?
            .error_for_status()
            .map_err(|e| Error::Http(e))?
            .bytes()
            .await
            .map_err(|e| Error::Http(e))?;

        let decoded = decode_codelist_bytes(&bytes)?;
        let codes = parse_codelist_tsv(&decoded)?;
        let mut code_lines = Vec::with_capacity(codes.len());
        for (code, label) in codes {
            code_lines.push(
                serde_json::to_string(&json!({
                    "codelist_id": codelist_id,
                    "code": code,
                    "label": label,
                }))
                .map_err(|e| Error::ClickHouse(e.to_string()))?,
            );
        }
        const BATCH: usize = 10_000;
        for chunk in code_lines.chunks(BATCH) {
            client.insert_json_lines("codes", chunk).await?;
        }
        status.codes += code_lines.len();
        info!(codelist_id = %codelist_id, count = code_lines.len(), "loaded codelist codes");
    }

    reload_dictionaries(client).await?;
    Ok(status)
}

async fn reload_dictionaries(client: &ClickHouseClient) -> Result<()> {
    for dict in ["dict_geo_labels", "dict_unit_labels"] {
        if let Err(err) = client.execute_sql(&format!("SYSTEM RELOAD DICTIONARY {dict}")).await {
            tracing::warn!(%dict, %err, "dictionary reload failed (may need auth fix in 002_schema.sql)");
        }
    }
    Ok(())
}

fn resolve_targets(options: &LoadCodelistsOptions, inventory: &[InventoryEntry]) -> Vec<String> {
    if options.all {
        return inventory.iter().map(|e| e.id.clone()).collect();
    }
    if let Some(ids) = &options.codelists {
        return ids.clone();
    }
    DEFAULT_CODELISTS.iter().map(|s| (*s).to_string()).collect()
}

struct CatalogueEntry {
    id: String,
    label: String,
    last_update: Option<String>,
    is_standard: u8,
}

struct InventoryEntry {
    id: String,
    label: String,
    latest_tsv_url: String,
}

fn parse_catalogue_tsv(path: &Path) -> Result<Vec<CatalogueEntry>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Io(e))?;
    let mut entries = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 || line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 4 {
            continue;
        }
        entries.push(CatalogueEntry {
            id: parts[0].trim().to_string(),
            label: parts[1].trim().to_string(),
            last_update: parse_catalogue_date(parts[2].trim()),
            is_standard: u8::from(parts[3].trim().eq_ignore_ascii_case("Y")),
        });
    }
    Ok(entries)
}

fn parse_inventory_tsv(path: &Path) -> Result<Vec<InventoryEntry>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Io(e))?;
    let mut entries = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 7 {
            continue;
        }
        entries.push(InventoryEntry {
            id: parts[0].trim().to_string(),
            label: parts[3].trim().to_string(),
            latest_tsv_url: parts[6].trim().to_string(),
        });
    }
    Ok(entries)
}

fn parse_catalogue_date(raw: &str) -> Option<String> {
    let date_part = raw.split('_').next().unwrap_or(raw).trim();
    if date_part.len() == 10 {
        Some(date_part.to_string())
    } else {
        None
    }
}

fn decode_codelist_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| Error::Parse(format!("gunzip codelist: {e}")))?;
        return Ok(out);
    }
    Ok(bytes.to_vec())
}

fn parse_codelist_tsv(bytes: &[u8]) -> Result<Vec<(String, String)>> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or_else(|| Error::Parse("codelist TSV missing header".into()))?;
    let columns: Vec<&str> = header.split('\t').collect();
    let code_idx = columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case("CODE"))
        .unwrap_or(0);
    let label_idx = columns
        .iter()
        .position(|c| c.starts_with("Label"))
        .unwrap_or(1);

    let mut codes = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() <= code_idx.max(label_idx) {
            continue;
        }
        let code = parts[code_idx].trim();
        let label = parts[label_idx].trim();
        if code.is_empty() {
            continue;
        }
        codes.push((code.to_string(), label.to_string()));
    }
    Ok(codes)
}
