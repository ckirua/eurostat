//! Error types for the Eurostat library.

use thiserror::Error;

/// Result type alias for library operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur when interacting with Eurostat APIs.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP transport or status error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// SQLite database error.
    #[cfg(feature = "cache")]
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// URL parsing error.
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),

    /// CSV parsing error.
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    /// Arrow/Parquet error.
    #[cfg(feature = "export")]
    #[error("arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// Parquet write error.
    #[cfg(feature = "export")]
    #[error("parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    /// XML parsing error.
    #[error("XML error: {0}")]
    Xml(String),

    /// API returned an error response.
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    /// Invalid configuration.
    #[error("configuration error: {0}")]
    Config(String),

    /// Dataset or resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Comext datasets require at least one filter.
    #[error("Comext dataset '{dataset}' requires at least one dimension filter")]
    ComextFilterRequired { dataset: String },

    /// Async job failed or timed out.
    #[error("async job error: {0}")]
    AsyncJob(String),

    /// Data format parsing error.
    #[error("parse error: {0}")]
    Parse(String),

    /// S3 object storage error.
    #[cfg(feature = "s3")]
    #[error("S3 error: {0}")]
    S3(String),

    /// ClickHouse database error.
    #[cfg(feature = "clickhouse")]
    #[error("ClickHouse error: {0}")]
    ClickHouse(String),
}

impl Error {
    /// Create an API error from status and body text.
    pub fn api(status: u16, message: impl Into<String>) -> Self {
        Self::Api {
            status,
            message: message.into(),
        }
    }
}
