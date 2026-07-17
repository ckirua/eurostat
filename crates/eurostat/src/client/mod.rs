//! HTTP client with retries and gzip support.

mod http;

use std::sync::Arc;

use crate::async_jobs::AsyncJobsClient;
#[cfg(feature = "bulk")]
use crate::bulk::BulkClient;
#[cfg(feature = "cache")]
use crate::cache::Cache;
use crate::catalogue::CatalogueClient;
use crate::comext::ComextClient;
use crate::config::Config;
use crate::error::Result;
use crate::sdmx::SdmxClient;
use crate::statistics::StatisticsClient;

pub use http::HttpClient;

/// Main entry point for Eurostat API access.
#[derive(Clone)]
pub struct EurostatClient {
    config: Arc<Config>,
    http: HttpClient,
    #[cfg(feature = "cache")]
    cache: Option<Cache>,
}

impl EurostatClient {
    /// Create a client with the given configuration.
    pub fn new(config: Config) -> Result<Self> {
        let http = HttpClient::new(&config)?;
        Ok(Self {
            config: Arc::new(config),
            http,
            #[cfg(feature = "cache")]
            cache: None,
        })
    }

    /// Create a client and open the SQLite cache.
    #[cfg(feature = "cache")]
    pub async fn with_cache(config: Config) -> Result<Self> {
        let http = HttpClient::new(&config)?;
        let cache = Cache::open(&config).await?;
        Ok(Self {
            config: Arc::new(config),
            http,
            cache: Some(cache),
        })
    }

    /// Access the active configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Access the underlying HTTP client.
    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Access the optional SQLite cache.
    #[cfg(feature = "cache")]
    pub fn cache(&self) -> Option<&Cache> {
        self.cache.as_ref()
    }

    /// Catalogue API client.
    pub fn catalogue(&self) -> CatalogueClient<'_> {
        CatalogueClient::new(self)
    }

    /// Statistics API client.
    pub fn statistics(&self) -> StatisticsClient<'_> {
        StatisticsClient::new(self)
    }

    /// SDMX 2.1/3.0 API client.
    pub fn sdmx(&self) -> SdmxClient<'_> {
        SdmxClient::new(self)
    }

    /// Comext-specific helpers.
    pub fn comext(&self) -> ComextClient<'_> {
        ComextClient::new(self)
    }

    /// Async job API client.
    pub fn async_jobs(&self) -> AsyncJobsClient<'_> {
        AsyncJobsClient::new(self)
    }

    /// Bulk download facility client.
    #[cfg(feature = "bulk")]
    pub fn bulk(&self) -> BulkClient<'_> {
        BulkClient::new(self)
    }

    /// Smoke-test connectivity by listing catalogue datasets.
    pub async fn ping(&self) -> Result<usize> {
        self.catalogue().list_datasets().await.map(|d| d.len())
    }
}
