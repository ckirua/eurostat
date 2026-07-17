//! Comext-specific helpers.

use crate::client::EurostatClient;
use crate::error::Result;
use crate::model::DatasetInfo;

/// Comext API helpers.
pub struct ComextClient<'a> {
    client: &'a EurostatClient,
}

impl<'a> ComextClient<'a> {
    /// Create a Comext client.
    pub fn new(client: &'a EurostatClient) -> Self {
        Self { client }
    }

    /// List Comext dataflows.
    pub async fn list_dataflows(&self) -> Result<Vec<DatasetInfo>> {
        let url = self
            .client
            .http()
            .comext_url("sdmx/2.1/dataflow/ESTAT/all?format=json");
        let payload: ComextDataflowResponse = self.client.http().get_json(&url).await?;
        Ok(payload.into_datasets())
    }
}

#[derive(Debug, serde::Deserialize)]
struct ComextDataflowResponse {
    #[serde(default)]
    dataflows: Vec<ComextDataflow>,
}

impl ComextDataflowResponse {
    fn into_datasets(self) -> Vec<DatasetInfo> {
        self.dataflows
            .into_iter()
            .map(|flow| {
                let id = flow.id;
                DatasetInfo {
                    title: flow.name.unwrap_or_else(|| id.clone()),
                    id,
                    description: flow.description,
                    updated_at: None,
                    url: None,
                }
            })
            .collect()
    }
}

#[derive(Debug, serde::Deserialize)]
struct ComextDataflow {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}
