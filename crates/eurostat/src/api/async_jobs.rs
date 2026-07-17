//! Async job API for large SDMX queries.

use std::time::Duration;

use tokio::time::sleep;
use tracing::debug;

use crate::client::EurostatClient;
use crate::error::{Error, Result};
use crate::model::{AsyncJob, JobStatus};

/// Client for Eurostat async request API.
pub struct AsyncJobsClient<'a> {
    client: &'a EurostatClient,
}

impl<'a> AsyncJobsClient<'a> {
    /// Create an async jobs client.
    pub fn new(client: &'a EurostatClient) -> Self {
        Self { client }
    }

    /// Poll job status until completion or timeout.
    pub async fn wait_for_completion(
        &self,
        job_id: &str,
        poll_interval: Duration,
        max_attempts: u32,
    ) -> Result<AsyncJob> {
        for attempt in 0..max_attempts {
            let job = self.status(job_id).await?;
            match job.status {
                JobStatus::Completed => return Ok(job),
                JobStatus::Failed => {
                    return Err(Error::AsyncJob(format!("job {job_id} failed")));
                }
                JobStatus::Pending
                | JobStatus::Submitted
                | JobStatus::Running
                | JobStatus::Unknown => {
                    sleep(poll_interval).await;
                }
            }
            if attempt + 1 == max_attempts {
                return Err(Error::AsyncJob(format!("job {job_id} timed out")));
            }
        }
        Err(Error::AsyncJob(format!("job {job_id} timed out")))
    }

    /// Fetch current job status.
    pub async fn status(&self, job_id: &str) -> Result<AsyncJob> {
        let url = self
            .client
            .http()
            .dissemination_url(&format!("1.0/async/status/{job_id}"));
        debug!(%url, "polling async job status");
        let payload: AsyncStatusResponse = self.client.http().get_json(&url).await?;
        Ok(AsyncJob {
            id: job_id.to_string(),
            status: payload.status,
            result_url: Some(
                self.client
                    .http()
                    .dissemination_url(&format!("1.0/async/data/{job_id}")),
            ),
        })
    }

    /// Download completed job result bytes.
    pub async fn download_result(&self, job: &AsyncJob) -> Result<Vec<u8>> {
        let url = job
            .result_url
            .as_ref()
            .ok_or_else(|| Error::AsyncJob(format!("job {} has no result URL", job.id)))?;
        self.client.http().get_bytes(url).await
    }
}

#[derive(Debug, serde::Deserialize)]
struct AsyncStatusResponse {
    #[serde(default = "default_status")]
    status: JobStatus,
}

fn default_status() -> JobStatus {
    JobStatus::Unknown
}
