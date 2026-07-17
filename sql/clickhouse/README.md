# ClickHouse schema

Apply in order on a fresh database:

```bash
# 1. Bootstrap (v1 tables)
clickhouse-client --multiquery < sql/clickhouse/001_schema.sql

# 2. Grafana-first v2 schema, views, dictionaries
clickhouse-client --multiquery < sql/clickhouse/002_schema.sql
```

If tables are empty, `002_schema.sql` alone is sufficient (it recreates `catalogue` and `observations`).

Verify:

```bash
clickhouse-client -d eurostat -q "SHOW TABLES"
```

See [docs/CLICKHOUSE_SCHEMA.md](../../docs/CLICKHOUSE_SCHEMA.md) for the full spec.  
Operations (ingest, cron, monitoring): [docs/DEPLOY.md](../../docs/DEPLOY.md).
