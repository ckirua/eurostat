//! SQLite cache for catalogue metadata.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tracing::info;

use crate::config::Config;
use crate::error::Result;
use crate::model::DatasetInfo;

/// SQLite-backed catalogue cache.
#[derive(Clone)]
pub struct Cache {
    pool: SqlitePool,
}

impl Cache {
    /// Open or create the cache database.
    pub async fn open(config: &Config) -> Result<Self> {
        let db_path = config.database_path()?;
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let cache = Self { pool };
        cache.migrate().await?;
        Ok(cache)
    }

    /// Upsert datasets into the cache.
    pub async fn upsert_datasets(&self, datasets: &[DatasetInfo]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for dataset in datasets {
            let raw_json = serde_json::to_string(dataset)?;
            sqlx::query(
                "INSERT INTO datasets (id, title, description, updated_at, raw_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                   title = excluded.title,
                   description = excluded.description,
                   updated_at = excluded.updated_at,
                   raw_json = excluded.raw_json",
            )
            .bind(&dataset.id)
            .bind(&dataset.title)
            .bind(&dataset.description)
            .bind(&dataset.updated_at)
            .bind(raw_json)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        info!(count = datasets.len(), "cache updated");
        Ok(())
    }

    /// Search datasets using FTS5.
    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<DatasetInfo>> {
        let rows = sqlx::query(
            "SELECT d.id, d.title, d.description, d.updated_at, d.raw_json
             FROM datasets_fts fts
             JOIN datasets d ON d.id = fts.id
             WHERE datasets_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_dataset).collect()
    }

    /// Fetch a dataset by id from cache.
    pub async fn get_dataset(&self, dataset_id: &str) -> Result<Option<DatasetInfo>> {
        let row = sqlx::query(
            "SELECT id, title, description, updated_at, raw_json FROM datasets WHERE id = ?1",
        )
        .bind(dataset_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_dataset).transpose()
    }

    /// Return all cached datasets.
    pub async fn all_datasets(&self) -> Result<Vec<DatasetInfo>> {
        let rows = sqlx::query(
            "SELECT id, title, description, updated_at, raw_json FROM datasets ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_dataset).collect()
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS datasets (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                updated_at TEXT,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS datasets_fts USING fts5(
                id UNINDEXED,
                title,
                description,
                content='datasets',
                content_rowid='rowid'
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS datasets_ai AFTER INSERT ON datasets BEGIN
                INSERT INTO datasets_fts(rowid, id, title, description)
                VALUES (new.rowid, new.id, new.title, COALESCE(new.description, ''));
             END",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS datasets_ad AFTER DELETE ON datasets BEGIN
                INSERT INTO datasets_fts(datasets_fts, rowid, id, title, description)
                VALUES ('delete', old.rowid, old.id, old.title, COALESCE(old.description, ''));
             END",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS datasets_au AFTER UPDATE ON datasets BEGIN
                INSERT INTO datasets_fts(datasets_fts, rowid, id, title, description)
                VALUES ('delete', old.rowid, old.id, old.title, COALESCE(old.description, ''));
                INSERT INTO datasets_fts(rowid, id, title, description)
                VALUES (new.rowid, new.id, new.title, COALESCE(new.description, ''));
             END",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

fn row_to_dataset(row: sqlx::sqlite::SqliteRow) -> Result<DatasetInfo> {
    let raw_json: String = row.try_get("raw_json")?;
    if let Ok(dataset) = serde_json::from_str::<DatasetInfo>(&raw_json) {
        return Ok(dataset);
    }
    Ok(DatasetInfo {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        updated_at: row.try_get("updated_at")?,
        url: None,
    })
}

/// Remove the cache database at `path` if it exists.
pub async fn remove_cache_at(path: &Path) -> Result<()> {
    if path.exists() {
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}
