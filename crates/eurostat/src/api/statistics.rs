//! Statistics API client (JSON-stat).

use std::collections::HashMap;

use tracing::debug;
use url::Url;

use crate::client::EurostatClient;
use crate::config::sanitize_dataset_id;
use crate::error::Result;
use crate::model::{DataFormat, ObservationTable};
use crate::parse::json_stat::parse_json_stat;

/// Client for the Eurostat Statistics API.
pub struct StatisticsClient<'a> {
    client: &'a EurostatClient,
}

impl<'a> StatisticsClient<'a> {
    /// Create a statistics client.
    pub fn new(client: &'a EurostatClient) -> Self {
        Self { client }
    }

    /// Fetch a dataset via the Statistics API.
    pub async fn fetch(
        &self,
        dataset_id: &str,
        filters: &HashMap<String, String>,
        language: Option<&str>,
    ) -> Result<ObservationTable> {
        let base = self.client.http().dissemination_url(&format!(
            "statistics/1.0/data/{}",
            sanitize_dataset_id(dataset_id)
        ));
        let mut url = Url::parse(&base).map_err(crate::error::Error::Url)?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("format", DataFormat::JsonStat.as_query_value());
            let lang = language.unwrap_or(&self.client.config().language);
            pairs.append_pair("lang", lang);
            for (key, value) in filters {
                pairs.append_pair(key, value);
            }
        }

        let url = url.to_string();
        debug!(%url, dataset_id, "fetching statistics data");
        let bytes = self.client.http().get_bytes(&url).await?;
        parse_json_stat(&bytes, dataset_id)
    }
}
