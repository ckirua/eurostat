//! Populate catalogue and dataset_dimensions tables during ingest.

use chrono::Utc;
use serde_json::json;

use crate::config::Config;
use crate::error::Result;
use crate::mirror::{dataset_prefix, s3_object_key};
use crate::model::DatasetInfo;

use super::client::ClickHouseClient;
use super::ingest::FEATURED_DATASETS;
use super::normalize::normalize_column_name;
use super::row::ParquetDataset;

/// Upsert catalogue and dataset_dimensions for an ingested dataset.
pub async fn upsert_catalogue_metadata(
    client: &ClickHouseClient,
    config: &Config,
    parquet: &ParquetDataset,
    row_count: u64,
) -> Result<()> {
    let dataset_id = &parquet.dataset_id;
    let info = dataset_info(config, dataset_id).await?;

    let dimension_ids = parquet
        .headers
        .iter()
        .filter(|h| !is_metadata_column(h))
        .cloned()
        .collect::<Vec<_>>();

    let (primary_geo, primary_time, primary_unit) =
        infer_primary_dims(&parquet.headers, parquet.rows.first());

    let freq_values = parquet
        .rows
        .iter()
        .filter_map(|r| r.freq.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let is_featured = u8::from(FEATURED_DATASETS.contains(&dataset_id.as_str()));
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let catalogue_line = serde_json::to_string(&json!({
        "dataset_id": dataset_id,
        "title": info.title,
        "prefix": dataset_prefix(dataset_id),
        "theme": serde_json::Value::Null,
        "s3_key": s3_object_key(dataset_id, "dissemination"),
        "row_count": row_count,
        "dimensions": dimension_ids,
        "primary_geo_dim": primary_geo,
        "primary_time_dim": primary_time,
        "primary_unit_dim": primary_unit,
        "freq_values": freq_values,
        "comparable_group": serde_json::Value::Null,
        "is_featured": is_featured,
        "last_eurostat_update": info.updated_at,
        "last_ingested_at": now,
    }))
    .map_err(|e| crate::error::Error::ClickHouse(e.to_string()))?;

    client
        .insert_json_lines("catalogue", &[catalogue_line])
        .await?;

    let mut dimension_lines = Vec::new();
    for (position, header) in dimension_ids.iter().enumerate() {
        let line = serde_json::to_string(&json!({
            "dataset_id": dataset_id,
            "dimension_id": header,
            "position": position,
            "label": serde_json::Value::Null,
            "codelist_id": serde_json::Value::Null,
        }))
        .map_err(|e| crate::error::Error::ClickHouse(e.to_string()))?;
        dimension_lines.push(line);
    }

    if !dimension_lines.is_empty() {
        client
            .insert_json_lines("dataset_dimensions", &dimension_lines)
            .await?;
    }

    Ok(())
}

async fn dataset_info(config: &Config, dataset_id: &str) -> Result<DatasetInfo> {
    #[cfg(feature = "cache")]
    {
        if let Ok(cache) = crate::api::cache::Cache::open(config).await {
            if let Some(info) = cache.get_dataset(dataset_id).await? {
                return Ok(info);
            }
        }
    }
    Ok(DatasetInfo {
        id: dataset_id.to_string(),
        title: dataset_id.to_string(),
        description: None,
        updated_at: None,
        url: None,
    })
}

fn is_metadata_column(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "value" | "values" | "obs_value" | "status"
    )
}

fn infer_primary_dims(
    headers: &[String],
    first_row: Option<&super::row::ObservationRow>,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut primary_geo = None;
    let mut primary_time = None;
    let mut primary_unit = None;

    for header in headers {
        match normalize_column_name(header) {
            Some("geo") if primary_geo.is_none() => primary_geo = Some(header.clone()),
            Some("time_code") if primary_time.is_none() => primary_time = Some(header.clone()),
            Some("unit") if primary_unit.is_none() => primary_unit = Some(header.clone()),
            _ => {}
        }
    }

    if first_row.is_some() {
        if primary_geo.is_none() && first_row.and_then(|r| r.geo.as_ref()).is_some() {
            primary_geo = headers
                .iter()
                .find(|h| normalize_column_name(h) == Some("geo"))
                .cloned();
        }
    }

    let _ = first_row;
    (primary_geo, primary_time, primary_unit)
}
