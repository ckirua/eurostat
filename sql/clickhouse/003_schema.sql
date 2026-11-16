-- Eurostat ClickHouse schema v3 — ingest resume by inventory change stamp
-- Apply after 002_schema.sql:
--   clickhouse-client --multiquery < sql/clickhouse/003_schema.sql

ALTER TABLE eurostat.ingest_log
    ADD COLUMN IF NOT EXISTS last_data_change Nullable(String) AFTER s3_row_count;
