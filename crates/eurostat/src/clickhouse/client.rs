//! ClickHouse client and batch insert helpers.

use chrono::NaiveDate;
use clickhouse::Client;
use serde_json::{json, Map, Value};

use crate::error::{Error, Result};

use super::config::ClickHouseConfig;
use super::row::ObservationRow;

/// Fields written to `eurostat.ingest_log`.
#[derive(Debug, Clone)]
pub struct IngestLogEntry<'a> {
    pub dataset_id: &'a str,
    pub s3_key: &'a str,
    pub s3_row_count: u64,
    pub row_count: u64,
    pub status: &'a str,
    pub error: Option<&'a str>,
    pub last_data_change: Option<&'a str>,
}

/// ClickHouse insert client.
pub struct ClickHouseClient {
    client: Client,
    http: reqwest::Client,
    http_url: String,
    user: String,
    password: String,
    database: String,
}

impl ClickHouseClient {
    /// Connect using configuration.
    pub fn new(config: &ClickHouseConfig) -> Result<Self> {
        let client = Client::default()
            .with_url(config.url())
            .with_user(&config.user)
            .with_password(&config.password)
            .with_database(&config.database);
        Ok(Self {
            client,
            http: reqwest::Client::new(),
            http_url: config.url(),
            user: config.user.clone(),
            password: config.password.clone(),
            database: config.database.clone(),
        })
    }

    /// Insert observation rows in batches via JSONEachRow.
    pub async fn insert_observations(&self, rows: &[ObservationRow]) -> Result<()> {
        const BATCH: usize = 50_000;
        for chunk in rows.chunks(BATCH) {
            let body = chunk
                .iter()
                .map(observation_to_json)
                .collect::<Result<Vec<_>>>()?
                .join("\n");
            self.insert_json_each_row("observations", &body).await?;
        }
        Ok(())
    }

    /// Append an ingest log entry (`status`: `ok`, `failed`, or `skipped`).
    pub async fn insert_ingest_log(&self, entry: IngestLogEntry<'_>) -> Result<()> {
        let line = serde_json::to_string(&json!({
            "dataset_id": entry.dataset_id,
            "s3_key": entry.s3_key,
            "s3_row_count": entry.s3_row_count,
            "row_count": entry.row_count,
            "status": entry.status,
            "error": entry.error,
            "last_data_change": entry.last_data_change,
        }))
        .map_err(|e| Error::ClickHouse(e.to_string()))?;
        self.insert_json_each_row("ingest_log", &line).await
    }

    /// Returns true when a successful ingest already covers this S3 snapshot.
    ///
    /// Prefer matching both `s3_row_count` and inventory `last_data_change` so
    /// value-only Eurostat revisions (same row count) still re-ingest after S3 refresh.
    /// When `last_data_change` is unknown, fall back to row-count matching.
    pub async fn is_ingested_current(
        &self,
        dataset_id: &str,
        s3_row_count: u64,
        last_data_change: Option<&str>,
    ) -> Result<bool> {
        if s3_row_count == 0 && last_data_change.is_none() {
            return self.is_ingested(dataset_id).await;
        }

        let count: u64 = if let Some(stamp) = last_data_change {
            self.client
                .query(&format!(
                    "SELECT count() FROM {db}.ingest_log \
                     WHERE dataset_id = ? AND status = 'ok' \
                       AND s3_row_count = ? AND last_data_change = ?",
                    db = self.database
                ))
                .bind(dataset_id)
                .bind(s3_row_count)
                .bind(stamp)
                .fetch_one()
                .await
                .map_err(|e| Error::ClickHouse(e.to_string()))?
        } else {
            self.client
                .query(&format!(
                    "SELECT count() FROM {db}.ingest_log \
                     WHERE dataset_id = ? AND status = 'ok' AND s3_row_count = ?",
                    db = self.database
                ))
                .bind(dataset_id)
                .bind(s3_row_count)
                .fetch_one()
                .await
                .map_err(|e| Error::ClickHouse(e.to_string()))?
        };
        Ok(count > 0)
    }

    /// Returns true when a successful ingest exists for the dataset.
    pub async fn is_ingested(&self, dataset_id: &str) -> Result<bool> {
        let count: u64 = self
            .client
            .query(&format!(
                "SELECT count() FROM {db}.ingest_log WHERE dataset_id = ? AND status = 'ok'",
                db = self.database
            ))
            .bind(dataset_id)
            .fetch_one()
            .await
            .map_err(|e| Error::ClickHouse(e.to_string()))?;
        Ok(count > 0)
    }

    /// Ping ClickHouse.
    pub async fn ping(&self) -> Result<()> {
        let _: u8 = self
            .client
            .query("SELECT 1")
            .fetch_one()
            .await
            .map_err(|e| Error::ClickHouse(e.to_string()))?;
        Ok(())
    }

    /// Execute a DDL/DML statement without a result set.
    pub async fn execute_sql(&self, sql: &str) -> Result<()> {
        self.client
            .query(sql)
            .execute()
            .await
            .map_err(|e| Error::ClickHouse(e.to_string()))?;
        Ok(())
    }

    async fn insert_json_each_row(&self, table: &str, body: &str) -> Result<()> {
        if body.is_empty() {
            return Ok(());
        }
        self.insert_json_lines(table, &[body.to_string()]).await
    }

    /// Insert one or more JSONEachRow lines into a table.
    pub async fn insert_json_lines(&self, table: &str, lines: &[String]) -> Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let body = lines.join("\n");
        let query = format!("INSERT INTO {}.{table} FORMAT JSONEachRow", self.database);
        let response = self
            .http
            .post(&self.http_url)
            .basic_auth(&self.user, Some(&self.password))
            .query(&[("query", query.as_str())])
            .body(body.to_string())
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| Error::ClickHouse(e.to_string()))?;
        if !response.status().is_success() {
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(Error::ClickHouse(text));
        }
        Ok(())
    }
}

fn observation_to_json(row: &ObservationRow) -> Result<String> {
    let mut dimensions = Map::new();
    for (key, value) in &row.dimensions {
        dimensions.insert(key.clone(), Value::String(value.clone()));
    }

    let value = json!({
        "dataset_id": row.dataset_id,
        "prefix": row.prefix,
        "geo": row.geo,
        "time_code": row.time_code,
        "time_start": row.time_start.map(format_date),
        "time_end": row.time_end.map(format_date),
        "freq": row.freq,
        "unit": row.unit,
        "dimensions": Value::Object(dimensions),
        "value": row.value,
        "status": row.status,
    });

    serde_json::to_string(&value).map_err(|e| Error::ClickHouse(e.to_string()))
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}
