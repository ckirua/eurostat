//! Integration tests for default data directory layout.

use std::path::PathBuf;

use eurostat::export::export_parquet;
use eurostat::parse::json_stat::parse_json_stat;
use eurostat::{Config, OutputFormat, DEFAULT_DATA_DIR};

#[test]
fn default_data_dir_name_is_eurostat_under_xdg() {
    assert_eq!(DEFAULT_DATA_DIR, "eurostat");
}

#[test]
fn data_dir_layout_matches_expected_structure() {
    let base = tempfile::tempdir().expect("tempdir");
    let config = Config {
        cache_dir: Some(base.path().to_path_buf()),
        ..Default::default()
    };

    config.ensure_data_dirs().expect("create dirs");

    assert!(base.path().join("exports").is_dir());
    assert!(base.path().join("bulk").is_dir());
    assert_eq!(
        config.database_path().expect("db path"),
        base.path().join("eurostat.db")
    );
}

#[test]
fn default_export_paths_for_csv_json_and_parquet() {
    let config = Config {
        cache_dir: Some(PathBuf::from("/data/eurostat")),
        ..Default::default()
    };

    assert_eq!(
        config
            .default_export_path("nama_10_gdp", OutputFormat::Csv)
            .expect("csv"),
        PathBuf::from("/data/eurostat/exports/nama_10_gdp.csv")
    );
    assert_eq!(
        config
            .default_export_path("nama_10_gdp", OutputFormat::Json)
            .expect("json"),
        PathBuf::from("/data/eurostat/exports/nama_10_gdp.json")
    );
    assert_eq!(
        config
            .default_export_path("nama_10_gdp", OutputFormat::Parquet)
            .expect("parquet"),
        PathBuf::from("/data/eurostat/exports/nama_10_gdp.parquet")
    );
}

#[cfg(feature = "export")]
#[test]
fn export_parquet_writes_to_default_exports_dir() {
    let base = tempfile::tempdir().expect("tempdir");
    let config = Config {
        cache_dir: Some(base.path().to_path_buf()),
        ..Default::default()
    };
    config.ensure_data_dirs().expect("dirs");

    let json = include_bytes!("fixtures/nama_10_gdp.json");
    let table = parse_json_stat(json, "nama_10_gdp").expect("parse");
    let path = config
        .default_export_path("nama_10_gdp", OutputFormat::Parquet)
        .expect("path");

    export_parquet(&table, &path).expect("export");
    assert!(path.exists());
    assert!(path.starts_with(base.path().join("exports")));
}
