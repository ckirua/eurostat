//! Catalogue API client.

use tracing::debug;

use crate::client::EurostatClient;
use crate::error::{Error, Result};
use crate::model::DatasetInfo;
use crate::parse::toc_xml;

/// Client for the Eurostat Catalogue API.
pub struct CatalogueClient<'a> {
    client: &'a EurostatClient,
}

impl<'a> CatalogueClient<'a> {
    /// Create a catalogue client.
    pub fn new(client: &'a EurostatClient) -> Self {
        Self { client }
    }

    /// List datasets from the catalogue TOC XML endpoint.
    pub async fn list_datasets(&self) -> Result<Vec<DatasetInfo>> {
        let lang = self.client.config().language.to_ascii_lowercase();
        let url = self
            .client
            .http()
            .dissemination_url(&format!("catalogue/toc/txt?lang={lang}"));
        debug!(%url, "listing catalogue datasets");
        let bytes = self.client.http().get_bytes(&url).await?;
        toc_xml::parse_toc_txt(&bytes)
    }

    /// Fetch metadata for a single dataset.
    pub async fn dataset_info(&self, dataset_id: &str) -> Result<DatasetInfo> {
        let datasets = self.list_datasets().await?;
        datasets
            .into_iter()
            .find(|d| d.id.eq_ignore_ascii_case(dataset_id))
            .ok_or_else(|| Error::NotFound(format!("dataset not found: {dataset_id}")))
    }
}
