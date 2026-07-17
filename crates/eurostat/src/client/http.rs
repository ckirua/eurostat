//! Reqwest-based HTTP client with retry and gzip handling.

use std::time::Duration;

use reqwest::{Client, Method, Response, StatusCode};
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::config::Config;
use crate::error::{Error, Result};

/// Low-level HTTP client used by API modules.
#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
    config: Config,
}

impl HttpClient {
    /// Build a configured HTTP client.
    pub fn new(config: &Config) -> Result<Self> {
        let inner = Client::builder()
            .timeout(config.timeout())
            .user_agent(&config.user_agent)
            .gzip(true)
            .build()?;
        Ok(Self {
            inner,
            config: config.clone(),
        })
    }

    /// Active configuration snapshot.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Perform a GET request with retries.
    pub async fn get(&self, url: &str) -> Result<Response> {
        self.request(Method::GET, url, None).await
    }

    /// Perform a GET request and return response bytes.
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.get(url).await?;
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Perform a GET request and deserialize JSON.
    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self.get(url).await?;
        let value = response.json().await?;
        Ok(value)
    }

    /// Perform a POST request with optional JSON body.
    pub async fn post_json<T: serde::Serialize>(&self, url: &str, body: &T) -> Result<Response> {
        let mut attempt = 0;
        loop {
            let response = self.inner.post(url).json(body).send().await;

            match response {
                Ok(resp)
                    if Self::should_retry(resp.status()) && attempt < self.config.max_retries =>
                {
                    attempt += 1;
                    warn!(status = %resp.status(), attempt, "retrying POST");
                    sleep(Self::backoff(self.config.retry_backoff_ms, attempt)).await;
                    continue;
                }
                Ok(resp) => return Self::check_response(resp).await,
                Err(err)
                    if attempt < self.config.max_retries && err.is_connect()
                        || err.is_timeout() =>
                {
                    attempt += 1;
                    warn!(attempt, error = %err, "retrying POST after transport error");
                    sleep(Self::backoff(self.config.retry_backoff_ms, attempt)).await;
                    continue;
                }
                Err(err) => return Err(Error::Http(err)),
            }
        }
    }

    /// Build a URL under the main dissemination API.
    pub fn dissemination_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// Build a URL under the Comext dissemination API.
    pub fn comext_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.comext_base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn request(
        &self,
        method: Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Response> {
        let mut attempt = 0;
        loop {
            debug!(%url, %method, attempt, "HTTP request");
            let mut builder = self.inner.request(method.clone(), url);
            if let Some(ref json) = body {
                builder = builder.json(json);
            }

            match builder.send().await {
                Ok(resp)
                    if Self::should_retry(resp.status()) && attempt < self.config.max_retries =>
                {
                    attempt += 1;
                    warn!(status = %resp.status(), attempt, "retrying request");
                    sleep(Self::backoff(self.config.retry_backoff_ms, attempt)).await;
                    continue;
                }
                Ok(resp) => return Self::check_response(resp).await,
                Err(err)
                    if attempt < self.config.max_retries
                        && (err.is_connect() || err.is_timeout()) =>
                {
                    attempt += 1;
                    warn!(attempt, error = %err, "retrying after transport error");
                    sleep(Self::backoff(self.config.retry_backoff_ms, attempt)).await;
                    continue;
                }
                Err(err) => return Err(Error::Http(err)),
            }
        }
    }

    async fn check_response(response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let message = response.text().await.unwrap_or_default();
        Err(Error::api(status.as_u16(), message))
    }

    fn should_retry(status: StatusCode) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS
            || status == StatusCode::REQUEST_TIMEOUT
            || status.is_server_error()
    }

    fn backoff(base_ms: u64, attempt: u32) -> Duration {
        Duration::from_millis(base_ms.saturating_mul(1_u64 << attempt.min(6)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_with_attempts() {
        assert_eq!(HttpClient::backoff(100, 1).as_millis(), 200);
        assert_eq!(HttpClient::backoff(100, 3).as_millis(), 800);
    }
}
