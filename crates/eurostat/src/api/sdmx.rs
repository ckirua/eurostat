//! SDMX 2.1 and 3.0 API client.

use std::collections::HashMap;

use tracing::debug;
use url::Url;

use crate::client::EurostatClient;
use crate::config::sanitize_dataset_id;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::{DataFormat, Dimension, ObservationTable};
use crate::parse::{json_stat, sdmx_csv, sdmx_xml, tsv};

/// SDMX protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdmxVersion {
    /// SDMX 2.1.
    V2_1,
    /// SDMX 3.0.
    V3_0,
}

/// Client for Eurostat SDMX APIs.
pub struct SdmxClient<'a> {
    client: &'a EurostatClient,
}

impl<'a> SdmxClient<'a> {
    /// Create an SDMX client.
    pub fn new(client: &'a EurostatClient) -> Self {
        Self { client }
    }

    /// Fetch dataset structure dimensions.
    pub async fn dimensions(
        &self,
        dataset_id: &str,
        version: SdmxVersion,
    ) -> Result<Vec<Dimension>> {
        let path = match version {
            SdmxVersion::V2_1 => {
                format!("sdmx/2.1/datastructure/ESTAT/{dataset_id}/?references=all")
            }
            SdmxVersion::V3_0 => {
                format!("sdmx/3.0/structure/datastructure/ESTAT/{dataset_id}/?references=all")
            }
        };

        let base = self.base_url_for(dataset_id);
        let url = format!("{base}/{path}");
        debug!(%url, "fetching SDMX structure");
        let bytes = self.client.http().get_bytes(&url).await?;
        sdmx_xml::parse_dimensions(&bytes)
    }

    /// Fetch dataset observations.
    pub async fn fetch(
        &self,
        dataset_id: &str,
        key: Option<&str>,
        filters: &HashMap<String, String>,
        format: DataFormat,
        version: SdmxVersion,
    ) -> Result<ObservationTable> {
        if Config::is_comext_dataset(dataset_id)
            && filters.is_empty()
            && key.unwrap_or("").is_empty()
        {
            return Err(Error::ComextFilterRequired {
                dataset: dataset_id.to_string(),
            });
        }

        let key = key.unwrap_or("");
        let version_path = match version {
            SdmxVersion::V2_1 => "sdmx/2.1",
            SdmxVersion::V3_0 => "sdmx/3.0",
        };

        let mut url = format!(
            "{}/{}/data/{}/{}",
            self.base_url_for(dataset_id),
            version_path,
            sanitize_dataset_id(dataset_id),
            sanitize_dataset_id(key)
        );
        let mut parsed = Url::parse(&url).map_err(crate::error::Error::Url)?;
        {
            let mut pairs = parsed.query_pairs_mut();
            pairs.append_pair("format", format.as_query_value());
            for (name, value) in filters {
                pairs.append_pair(name, value);
            }
        }
        let url = parsed.to_string();

        debug!(%url, "fetching SDMX data");
        let bytes = self.client.http().get_bytes(&url).await?;

        match format {
            DataFormat::JsonStat => json_stat::parse_json_stat(&bytes, dataset_id),
            DataFormat::Tsv => tsv::parse_tsv(&bytes, dataset_id),
            DataFormat::SdmxCsv => sdmx_csv::parse_sdmx_csv(&bytes, dataset_id),
            DataFormat::SdmxXml => sdmx_xml::parse_data(&bytes, dataset_id),
        }
    }

    fn base_url_for(&self, dataset_id: &str) -> &str {
        self.client.config().base_url_for_dataset(dataset_id)
    }
}
