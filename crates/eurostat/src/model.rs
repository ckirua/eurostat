//! Shared data models.

use serde::{Deserialize, Serialize};

/// Catalogue dataset metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatasetInfo {
    /// Dataset code (e.g. `nama_10_gdp`).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// Last update timestamp from Eurostat, if known.
    pub updated_at: Option<String>,
    /// Source URL.
    pub url: Option<String>,
}

/// A dimension in a statistical dataset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dimension {
    /// Dimension code (e.g. `geo`, `time`).
    pub id: String,
    /// Display label.
    pub label: Option<String>,
    /// Allowed codes for this dimension.
    pub codes: Vec<DimensionCode>,
}

/// A code within a dimension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DimensionCode {
    /// Code value.
    pub id: String,
    /// Human-readable label.
    pub label: Option<String>,
}

/// A single observation row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    /// Dimension values keyed by dimension id.
    pub dimensions: Vec<(String, String)>,
    /// Observed value, if present.
    pub value: Option<f64>,
    /// Status flag (e.g. Eurostat flags).
    pub status: Option<String>,
}

/// Tabular dataset with dimensions and observations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ObservationTable {
    /// Dataset identifier.
    pub dataset_id: String,
    /// Dataset title.
    pub title: Option<String>,
    /// Dimension metadata.
    pub dimensions: Vec<Dimension>,
    /// Observations.
    pub observations: Vec<Observation>,
}

impl ObservationTable {
    /// Number of observations.
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Returns true when there are no observations.
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }
}

/// Output format for exporters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Comma-separated values.
    Csv,
    /// JSON array of flat records.
    Json,
    /// Apache Parquet.
    Parquet,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            "parquet" => Ok(Self::Parquet),
            _ => Err(format!("unsupported format: {s}")),
        }
    }
}

/// SDMX or Statistics API response format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    /// JSON-stat 2.0.
    JsonStat,
    /// Tab-separated values.
    Tsv,
    /// SDMX-CSV flat format.
    SdmxCsv,
    /// SDMX-ML XML.
    SdmxXml,
}

impl DataFormat {
    /// Query parameter value for Eurostat APIs.
    pub fn as_query_value(self) -> &'static str {
        match self {
            Self::JsonStat => "JSON",
            Self::Tsv => "TSV",
            Self::SdmxCsv => "SDMX-CSV",
            Self::SdmxXml => "SDMX-ML",
        }
    }
}

/// Bulk download file descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkFile {
    /// Dataset or archive code.
    pub dataset_id: String,
    /// Remote download URL.
    pub url: String,
    /// File format label.
    pub format: String,
    /// Optional compressed size in bytes.
    pub size_bytes: Option<u64>,
}

/// Async job status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    /// Job accepted and queued.
    #[default]
    Pending,
    /// Job submitted to queue.
    Submitted,
    /// Job is running.
    Running,
    /// Job completed successfully.
    Completed,
    /// Job failed.
    Failed,
    /// Unknown status from API.
    Unknown,
}

/// Async job handle returned by Eurostat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AsyncJob {
    /// Job identifier.
    pub id: String,
    /// Current status.
    pub status: JobStatus,
    /// Result download URL when completed.
    pub result_url: Option<String>,
}
