//! ClickHouse ingest — config, normalization, and load pipeline.

mod catalogue;
mod client;
mod codelists;
mod config;
mod ingest;
mod normalize;
mod parquet;
mod row;
mod time;

pub use client::ClickHouseClient;
pub use codelists::{load_codelists, LoadCodelistsOptions, LoadCodelistsStatus, DEFAULT_CODELISTS};
pub use config::ClickHouseConfig;
pub use ingest::{run_ingest, IngestOptions, IngestStatus, FEATURED_DATASETS};
pub use normalize::{normalize_column_name, normalize_row, NormalizedRow, PromotedColumns};
pub use parquet::{fetch_s3_parquet_dataset, parse_parquet_bytes};
pub use row::{ObservationRow, ParquetDataset};
pub use time::{parse_time_period, TimePeriod};
