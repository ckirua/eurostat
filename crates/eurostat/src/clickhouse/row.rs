//! Observation row matching the ClickHouse `observations` v2 schema.

use std::collections::HashMap;

use chrono::NaiveDate;

use super::normalize::NormalizedRow;
use crate::mirror::dataset_prefix;

/// One observation ready for ClickHouse insert.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationRow {
    pub dataset_id: String,
    pub prefix: String,
    pub geo: Option<String>,
    pub time_code: Option<String>,
    pub time_start: Option<NaiveDate>,
    pub time_end: Option<NaiveDate>,
    pub freq: Option<String>,
    pub unit: Option<String>,
    pub dimensions: HashMap<String, String>,
    pub value: Option<f64>,
    pub status: Option<String>,
}

impl ObservationRow {
    /// Build from a normalized parquet row.
    pub fn from_normalized(dataset_id: impl Into<String>, normalized: NormalizedRow) -> Self {
        let dataset_id = dataset_id.into();
        let prefix = dataset_prefix(&dataset_id);
        Self {
            dataset_id,
            prefix,
            geo: normalized.geo,
            time_code: normalized.time_code,
            time_start: normalized.time_start,
            time_end: normalized.time_end,
            freq: normalized.freq,
            unit: normalized.unit,
            dimensions: normalized.dimensions,
            value: normalized.value,
            status: normalized.status,
        }
    }
}

/// Parsed parquet dataset with column headers and observation rows.
#[derive(Debug, Clone, PartialEq)]
pub struct ParquetDataset {
    pub dataset_id: String,
    pub headers: Vec<String>,
    pub rows: Vec<ObservationRow>,
}
