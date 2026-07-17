//! Client configuration.

use std::path::PathBuf;
use std::time::Duration;

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::OutputFormat;

/// Default Eurostat dissemination API base URL.
pub const DEFAULT_BASE_URL: &str = "https://ec.europa.eu/eurostat/api/dissemination";

/// Default Comext API base URL.
pub const DEFAULT_COMEXT_BASE_URL: &str = "https://ec.europa.eu/eurostat/api/comext/dissemination";

/// Default bulk download index URL.
pub const DEFAULT_BULK_INDEX_URL: &str =
    "https://ec.europa.eu/eurostat/estat-navtree-portlet-prod/BulkDownloadListing";

/// Default user agent sent with HTTP requests.
pub const DEFAULT_USER_AGENT: &str = concat!("eurostat/", env!("CARGO_PKG_VERSION"));

/// Default application directory name under the XDG data home
/// (`~/.local/share` on Linux → `~/.local/share/eurostat`).
pub const DEFAULT_DATA_DIR: &str = "eurostat";

/// Runtime configuration for [`crate::EurostatClient`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Main Eurostat API base URL.
    pub base_url: String,
    /// Comext API base URL for `DS-*` datasets.
    pub comext_base_url: String,
    /// Bulk download facility index URL.
    pub bulk_index_url: String,
    /// HTTP request timeout in seconds.
    pub timeout_secs: u64,
    /// Maximum retry attempts for transient failures.
    pub max_retries: u32,
    /// Initial retry backoff in milliseconds.
    pub retry_backoff_ms: u64,
    /// User agent header value.
    pub user_agent: String,
    /// Preferred response language (`EN`, `FR`, or `DE`).
    pub language: String,
    /// Root data directory (SQLite cache, exports, bulk downloads).
    #[serde(skip)]
    pub cache_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            comext_base_url: DEFAULT_COMEXT_BASE_URL.to_string(),
            bulk_index_url: DEFAULT_BULK_INDEX_URL.to_string(),
            timeout_secs: 120,
            max_retries: 3,
            retry_backoff_ms: 500,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            language: "EN".to_string(),
            cache_dir: default_cache_dir(),
        }
    }
}

impl Config {
    /// Load configuration from the default file path, falling back to defaults.
    pub fn load() -> Result<Self> {
        let path = default_config_path();
        if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            let mut config: Self = toml::from_str(&contents)
                .map_err(|e| Error::Config(format!("invalid config file: {e}")))?;
            if config.cache_dir.is_none() {
                config.cache_dir = default_cache_dir();
            }
            return Ok(config);
        }
        Ok(Self::default())
    }

    /// Request timeout as a [`Duration`].
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// Resolved cache directory.
    pub fn cache_dir(&self) -> Result<PathBuf> {
        self.cache_dir
            .clone()
            .or_else(default_cache_dir)
            .ok_or_else(|| Error::Config("unable to resolve cache directory".into()))
    }

    /// SQLite database path inside the data directory.
    pub fn database_path(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("eurostat.db"))
    }

    /// Resolved root data directory (`~/.local/share/eurostat` by default).
    pub fn data_dir(&self) -> Result<PathBuf> {
        self.cache_dir()
    }

    /// Directory for exported datasets (Parquet, CSV, JSON files).
    pub fn exports_dir(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("exports"))
    }

    /// Directory for bulk downloads.
    pub fn bulk_dir(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("bulk"))
    }

    /// Directory for mirrored Arrow parquet.zstd datasets.
    pub fn datasets_dir(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("datasets"))
    }

    /// Directory for COMEXT mirror outputs.
    pub fn datasets_comext_dir(&self) -> Result<PathBuf> {
        Ok(self.datasets_dir()?.join("comext"))
    }

    /// Mirror progress log.
    pub fn manifest_path(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("manifest.jsonl"))
    }

    /// Default export path for a dataset and format under [`Self::exports_dir`].
    pub fn default_export_path(&self, dataset_id: &str, format: OutputFormat) -> Result<PathBuf> {
        let ext = match format {
            OutputFormat::Csv => "csv",
            OutputFormat::Json => "json",
            OutputFormat::Parquet => "parquet",
        };
        Ok(self
            .exports_dir()?
            .join(format!("{}.{}", safe_filename(dataset_id), ext)))
    }

    /// Create the data directory tree if it does not exist.
    pub fn ensure_data_dirs(&self) -> Result<()> {
        for dir in [
            self.data_dir()?,
            self.exports_dir()?,
            self.bulk_dir()?,
            self.datasets_dir()?,
            self.datasets_comext_dir()?,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Returns true when the dataset code belongs to Comext/PRODCOM.
    pub fn is_comext_dataset(dataset: &str) -> bool {
        dataset.starts_with("DS-") || dataset.starts_with("DS_")
    }

    /// Select the appropriate base URL for a dataset code.
    pub fn base_url_for_dataset(&self, dataset: &str) -> &str {
        if Self::is_comext_dataset(dataset) {
            &self.comext_base_url
        } else {
            &self.base_url
        }
    }
}

fn default_cache_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("EUROSTAT_DATA_DIR") {
        return Some(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("EUROSTAT_CACHE_DIR") {
        return Some(PathBuf::from(value));
    }
    // XDG data home on Linux/macOS: ~/.local/share/eurostat
    BaseDirs::new().map(|dirs| dirs.data_dir().join(DEFAULT_DATA_DIR))
}

fn default_config_path() -> PathBuf {
    if let Ok(value) = std::env::var("EUROSTAT_CONFIG") {
        return PathBuf::from(value);
    }
    default_cache_dir()
        .map(|dir| dir.join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

fn safe_filename(dataset_id: &str) -> String {
    sanitize_dataset_id(dataset_id)
}

/// Sanitize a dataset id for filesystem paths and S3 object keys.
pub(crate) fn sanitize_dataset_id(dataset_id: &str) -> String {
    dataset_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// Minimal TOML parsing without adding a heavy dependency at workspace level.
mod toml {
    use std::path::PathBuf;

    use super::{Config, Error, Result};

    pub fn from_str(input: &str) -> Result<Config> {
        let mut config = Config::default();
        for line in input.lines() {
            let line = line.split('#').next().unwrap_or(line).trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "base_url" => config.base_url = value.to_string(),
                "comext_base_url" => config.comext_base_url = value.to_string(),
                "bulk_index_url" => config.bulk_index_url = value.to_string(),
                "timeout_secs" => {
                    config.timeout_secs = value
                        .parse()
                        .map_err(|_| Error::Config(format!("invalid timeout_secs: {value}")))?;
                }
                "max_retries" => {
                    config.max_retries = value
                        .parse()
                        .map_err(|_| Error::Config(format!("invalid max_retries: {value}")))?;
                }
                "retry_backoff_ms" => {
                    config.retry_backoff_ms = value
                        .parse()
                        .map_err(|_| Error::Config(format!("invalid retry_backoff_ms: {value}")))?;
                }
                "user_agent" => config.user_agent = value.to_string(),
                "language" => config.language = value.to_string(),
                "cache_dir" | "data_dir" => config.cache_dir = Some(PathBuf::from(value)),
                _ => {}
            }
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_comext_datasets() {
        assert!(Config::is_comext_dataset("DS-057555"));
        assert!(!Config::is_comext_dataset("nama_10_gdp"));
    }

    #[test]
    fn selects_base_url_for_dataset() {
        let config = Config::default();
        assert_eq!(config.base_url_for_dataset("nama_10_gdp"), DEFAULT_BASE_URL);
        assert_eq!(
            config.base_url_for_dataset("DS-057555"),
            DEFAULT_COMEXT_BASE_URL
        );
    }

    #[test]
    fn default_export_path_uses_exports_subdir() {
        let config = Config {
            cache_dir: Some(PathBuf::from("/tmp/eurostat-test")),
            ..Default::default()
        };
        let path = config
            .default_export_path("nama_10_gdp", OutputFormat::Parquet)
            .expect("path");
        assert_eq!(
            path,
            PathBuf::from("/tmp/eurostat-test/exports/nama_10_gdp.parquet")
        );
    }

    #[test]
    fn safe_filename_replaces_invalid_chars() {
        assert_eq!(safe_filename("DS-057555"), "DS-057555");
        assert_eq!(safe_filename("weird/id"), "weird_id");
    }

    #[test]
    fn ensure_data_dirs_creates_exports_and_bulk() {
        let base = tempfile::tempdir().expect("tempdir");
        let config = Config {
            cache_dir: Some(base.path().to_path_buf()),
            ..Default::default()
        };
        config.ensure_data_dirs().expect("dirs");
        assert!(base.path().join("exports").is_dir());
        assert!(base.path().join("bulk").is_dir());
    }

    #[test]
    fn database_and_bulk_paths_are_under_data_dir() {
        let config = Config {
            cache_dir: Some(PathBuf::from("/tmp/eurostat-test")),
            ..Default::default()
        };
        assert_eq!(
            config.database_path().expect("db"),
            PathBuf::from("/tmp/eurostat-test/eurostat.db")
        );
        assert_eq!(
            config.bulk_dir().expect("bulk"),
            PathBuf::from("/tmp/eurostat-test/bulk")
        );
    }

    #[test]
    fn config_toml_parses_data_dir() {
        let config = toml::from_str("data_dir = \"/custom/eurostat\"\n").expect("parse");
        assert_eq!(config.cache_dir, Some(PathBuf::from("/custom/eurostat")));
    }
}
