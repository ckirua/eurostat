//! Rust library for interacting with Eurostat and Comext APIs.
//!
//! # Overview
//!
//! This crate provides async clients for:
//! - [Catalogue API](https://ec.europa.eu/eurostat/web/user-guides/data-browser/api-data-access/api-getting-started)
//! - Statistics API (JSON-stat)
//! - SDMX 2.1 and 3.0 APIs
//! - Comext datasets (`DS-*` codes)
//! - Async jobs for large queries
//! - Bulk download facility
//!
//! # Fast iteration during development
//!
//! Skip the heaviest dependencies when you only need HTTP/parsing:
//!
//! ```bash
//! cargo check -p eurostat --no-default-features
//! ```
//!
//! Enable features as needed: `cache`, `export`, `bulk`.
//!
//! # Example
//!
//! ```no_run
//! use eurostat::{Config, EurostatClient};
//!
//! # async fn example() -> eurostat::Result<()> {
//! let client = EurostatClient::new(Config::default())?;
//! let datasets = client.catalogue().list_datasets().await?;
//! println!("{} datasets available", datasets.len());
//! # Ok(())
//! # }
//! ```

pub mod api;
pub mod client;
pub mod config;
pub mod error;
pub mod model;
pub mod parse;

#[cfg(all(feature = "bulk", feature = "export"))]
pub mod mirror;

#[cfg(feature = "clickhouse")]
pub mod clickhouse;

pub use api::async_jobs;
#[cfg(feature = "bulk")]
pub use api::bulk;
#[cfg(feature = "cache")]
pub use api::cache;
pub use api::catalogue;
pub use api::comext;
#[cfg(feature = "export")]
pub use api::export;
pub use api::sdmx;
#[cfg(feature = "cache")]
pub use api::search;
pub use api::statistics;

pub use client::EurostatClient;
pub use config::{Config, DEFAULT_DATA_DIR};
pub use error::{Error, Result};
#[cfg(feature = "s3")]
pub use mirror::{mirror_status_s3, run_s3_mirror, S3Config, S3_MANIFEST_KEY};
#[cfg(all(feature = "bulk", feature = "export"))]
pub use mirror::{
    dataset_prefix, mirror_status, run_mirror, s3_object_key, select_files, MirrorOptions,
    MirrorSource, MirrorStatus,
};
pub use model::OutputFormat;
#[cfg(feature = "clickhouse")]
pub use clickhouse::{
    fetch_s3_parquet_dataset, load_codelists, parse_parquet_bytes, parse_time_period, run_ingest,
    ClickHouseClient, ClickHouseConfig, IngestOptions, IngestStatus, LoadCodelistsOptions,
    LoadCodelistsStatus, ObservationRow, ParquetDataset, FEATURED_DATASETS, DEFAULT_CODELISTS,
};
#[cfg(feature = "cache")]
pub use search::SearchEngine;
