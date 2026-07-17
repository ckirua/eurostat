-- Eurostat ClickHouse schema v2 (Grafana-first, unified warehouse)
--
-- Prerequisite: 001_schema.sql applied.
-- Safe on empty tables: drops and recreates observations + catalogue.
--
-- Apply:
--   clickhouse-client --multiquery < sql/clickhouse/002_schema.sql
--
-- See docs/CLICKHOUSE_SCHEMA.md for time parsing rules and ingest contract.

CREATE DATABASE IF NOT EXISTS eurostat;

-- ---------------------------------------------------------------------------
-- catalogue v2 — dataset directory for Grafana variables & cross-dataset nav
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS eurostat.catalogue;

CREATE TABLE eurostat.catalogue
(
    dataset_id          LowCardinality(String),
    title               String,
    prefix              LowCardinality(String),
    theme               LowCardinality(Nullable(String)),
    s3_key              String,
    row_count           Nullable(UInt64),
    dimensions          Array(String),
    primary_geo_dim     LowCardinality(Nullable(String)),
    primary_time_dim    LowCardinality(Nullable(String)),
    primary_unit_dim    LowCardinality(Nullable(String)),
    freq_values         Array(LowCardinality(String)),
    comparable_group    LowCardinality(Nullable(String)),
    is_featured         UInt8 DEFAULT 0,
    last_eurostat_update Nullable(DateTime),
    last_ingested_at    Nullable(DateTime),
    updated_at          DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY dataset_id;

-- ---------------------------------------------------------------------------
-- dataset_dimensions — schema registry (one row per dimension per dataset)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS eurostat.dataset_dimensions
(
    dataset_id    LowCardinality(String),
    dimension_id  LowCardinality(String),
    position      UInt8,
    label         Nullable(String),
    codelist_id   LowCardinality(Nullable(String)),
    updated_at    DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (dataset_id, position);

-- ---------------------------------------------------------------------------
-- codelists + codes — human-readable labels (from public/codelists/)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS eurostat.codelists
(
    codelist_id   LowCardinality(String),
    label         String,
    last_update   Nullable(Date),
    is_standard   UInt8 DEFAULT 1,
    updated_at    DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY codelist_id;

CREATE TABLE IF NOT EXISTS eurostat.codes
(
    codelist_id   LowCardinality(String),
    code          String,
    label         String,
    updated_at    DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (codelist_id, code);

-- ---------------------------------------------------------------------------
-- observations v2 — unified fact table
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS eurostat.observations;

CREATE TABLE eurostat.observations
(
    dataset_id   LowCardinality(String),
    prefix       LowCardinality(String),

    -- Promoted & normalized at ingest (Grafana time-series / maps)
    geo          LowCardinality(Nullable(String)),
    time_code    LowCardinality(Nullable(String)),
    time_start   Nullable(Date),
    time_end     Nullable(Date),
    freq         LowCardinality(Nullable(String)),
    unit         LowCardinality(Nullable(String)),

    -- Dataset-specific dimensions (na_item, indic, sex, age, …)
    dimensions   Map(String, String),

    value        Nullable(Float64),
    status       LowCardinality(Nullable(String)),

    ingested_at  DateTime DEFAULT now(),
    revision     UInt32 DEFAULT 1
)
ENGINE = ReplacingMergeTree(ingested_at)
PARTITION BY prefix
ORDER BY (
    dataset_id,
    coalesce(geo, ''),
    coalesce(time_start, toDate(0)),
    cityHash64(toString(dimensions))
)
SETTINGS index_granularity = 8192;

-- ---------------------------------------------------------------------------
-- ingest_log v2 — resume + weekly refresh change detection
-- ---------------------------------------------------------------------------
ALTER TABLE eurostat.ingest_log
    ADD COLUMN IF NOT EXISTS s3_row_count Nullable(UInt64) AFTER s3_key;

ALTER TABLE eurostat.ingest_log
    ADD COLUMN IF NOT EXISTS duration_ms Nullable(UInt64) AFTER error;

-- ---------------------------------------------------------------------------
-- dictionaries — fast label lookups in Grafana queries
-- Populated after codes table has data.
-- ---------------------------------------------------------------------------
CREATE DICTIONARY IF NOT EXISTS eurostat.dict_geo_labels
(
    code  String,
    label String
)
PRIMARY KEY code
SOURCE(CLICKHOUSE(
    HOST 'localhost'
    PORT 9000
    USER 'default'
    TABLE 'codes'
    DB 'eurostat'
    WHERE 'codelist_id = \'GEO\''
))
LIFETIME(MIN 3600 MAX 86400)
LAYOUT(COMPLEX_KEY_HASHED());

CREATE DICTIONARY IF NOT EXISTS eurostat.dict_unit_labels
(
    code  String,
    label String
)
PRIMARY KEY code
SOURCE(CLICKHOUSE(
    HOST 'localhost'
    PORT 9000
    USER 'default'
    TABLE 'codes'
    DB 'eurostat'
    WHERE 'codelist_id = \'UNIT\''
))
LIFETIME(MIN 3600 MAX 86400)
LAYOUT(COMPLEX_KEY_HASHED());

-- ---------------------------------------------------------------------------
-- Grafana serving views
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW eurostat.v_timeseries AS
SELECT
    o.dataset_id,
    c.title                                              AS dataset_title,
    o.geo,
    dictGetOrDefault('eurostat.dict_geo_labels', 'label', o.geo, o.geo) AS geo_label,
    o.time_start                                         AS time,
    o.time_code,
    o.time_end,
    o.freq,
    o.unit,
    dictGetOrDefault('eurostat.dict_unit_labels', 'label', o.unit, o.unit) AS unit_label,
    o.value,
    o.status,
    o.dimensions,
    o.ingested_at
FROM eurostat.observations AS o
LEFT JOIN eurostat.catalogue AS c USING (dataset_id)
WHERE o.time_start IS NOT NULL
  AND o.value IS NOT NULL;

CREATE OR REPLACE VIEW eurostat.v_geo_map AS
SELECT
    dataset_id,
    geo,
    dictGetOrDefault('eurostat.dict_geo_labels', 'label', geo, geo) AS geo_label,
    time_start AS time,
    time_code,
    freq,
    value,
    status
FROM eurostat.observations
WHERE geo IS NOT NULL
  AND time_start IS NOT NULL
  AND value IS NOT NULL;

CREATE OR REPLACE VIEW eurostat.v_dataset_picker AS
SELECT
    dataset_id,
    title,
    prefix,
    theme,
    dimensions,
    freq_values,
    comparable_group,
    is_featured,
    row_count,
    last_ingested_at
FROM eurostat.catalogue
ORDER BY is_featured DESC, title;

-- Hero datasets for first Grafana dashboards (seed catalogue flags)
-- Ingest populates full catalogue; these are curation hints only.
CREATE OR REPLACE VIEW eurostat.v_featured_datasets AS
SELECT *
FROM eurostat.v_dataset_picker
WHERE is_featured = 1
   OR dataset_id IN (
        'nama_10_gdp',
        'prc_hicp_manr',
        'une_rt_m',
        'demo_pjan',
        'bop_c6_q'
   );
