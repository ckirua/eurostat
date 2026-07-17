-- Eurostat ClickHouse schema (v1)
-- Apply: clickhouse-client --multiquery < sql/clickhouse/001_schema.sql

CREATE DATABASE IF NOT EXISTS eurostat;

-- Dataset catalogue (metadata for dashboards / dataset picker)
CREATE TABLE IF NOT EXISTS eurostat.catalogue
(
    dataset_id   LowCardinality(String),
    title        String,
    prefix       LowCardinality(String),
    s3_key       String,
    row_count    Nullable(UInt64),
    last_synced  Nullable(DateTime),
    dimensions   Array(String),
    updated_at   DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY dataset_id;

-- Normalized observation facts (one row per observation)
CREATE TABLE IF NOT EXISTS eurostat.observations
(
    dataset_id  LowCardinality(String),
    dimensions  Map(String, String),
    value       Nullable(Float64),
    status      LowCardinality(Nullable(String)),
    ingested_at DateTime DEFAULT now()
)
ENGINE = MergeTree
PARTITION BY dataset_id
ORDER BY (dataset_id, cityHash64(toString(dimensions)))
SETTINGS index_granularity = 8192;

-- Ingest progress / resume ledger
CREATE TABLE IF NOT EXISTS eurostat.ingest_log
(
    dataset_id  LowCardinality(String),
    s3_key      String,
    row_count   UInt64,
    status      Enum8('ok' = 1, 'failed' = 2, 'skipped' = 3),
    error       Nullable(String),
    ingested_at DateTime DEFAULT now()
)
ENGINE = MergeTree
ORDER BY (dataset_id, ingested_at);
