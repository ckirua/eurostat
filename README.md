# Eurostat Rust CLI & Library

Production-grade Rust library and CLI for [Eurostat](https://ec.europa.eu/eurostat) and Comext APIs.

## Features

- **Catalogue API** — dataset discovery via TOC
- **Statistics API** — JSON-stat 2.0 queries
- **SDMX 2.1 / 3.0** — structure and data queries
- **Comext** — `DS-*` datasets with automatic base-URI routing
- **Async jobs** — large query polling (`/1.0/async/status/{uuid}`)
- **Bulk download** — inventory API with legacy HTML fallback
- **SQLite cache** — FTS5 search with optional fuzzy matching
- **Export** — CSV, JSON, Parquet (Arrow), ZSTD parquet
- **S3 mirror** — fetch → parquet.zstd → upload (any S3-compatible storage)

## Quick start (server → S3 → ClickHouse)

| Step | Guide |
|------|-------|
| **Configuration (env vars)** | **[docs/CONFIGURATION.md](docs/CONFIGURATION.md)** |
| **One-line install** | below, or **[docs/SERVER.md](docs/SERVER.md)** |
| ClickHouse ingest + cron | **[docs/DEPLOY.md](docs/DEPLOY.md)** |

```bash
# One-line install (Rust if needed, clone to ~/eurostat, build CLI)
curl -fsSL https://raw.githubusercontent.com/ckirua/eurostat/main/scripts/install.sh | bash

# Or clone and install from source
git clone https://github.com/ckirua/eurostat.git
cd eurostat
./scripts/install.sh --dir .

# Configure: add S3_EUROSTAT_BUCKET + S3_URL / S3_ACCESS_KEY / S3_SECRET_KEY / CLICKHOUSE_* to ~/.env
./scripts/sync-s3.sh
./scripts/clickhouse-ingest.sh   # after schema + S3 coverage
```

## Install

```bash
# From a git clone
./scripts/install.sh              # ~/eurostat by default
./scripts/install.sh --dir .      # install in current repo
./scripts/install.sh --global     # also put eurostat in ~/.cargo/bin
./scripts/install.sh --cron       # also install weekly cron jobs
./scripts/install.sh --update     # pull + rebuild existing clone

# Manual
cargo install --path crates/eurostat-cli
# or
make release   # → target/release/eurostat
```

Requires Rust 1.85+.

## Configuration

Secrets and connection settings go in **`~/.env`** (never commit it). Use [`.env.example`](.env.example) as a reference for which keys to add — append or edit in place; never replace the whole file.

| You want to… | Set these |
|--------------|-----------|
| Fetch / search / export only | Nothing required |
| Mirror to S3-compatible storage | `S3_EUROSTAT_BUCKET` + shared `S3_*` |
| Ingest into ClickHouse | `S3_EUROSTAT_BUCKET` + shared `S3_*` + `CLICKHOUSE_*` |

Full variable reference, local vs remote ClickHouse, and TLS: **[docs/CONFIGURATION.md](docs/CONFIGURATION.md)**.

| Variable | When needed | Default | Purpose |
|----------|-------------|---------|---------|
| `S3_ACCESS_KEY` | S3 | — | Access key |
| `S3_SECRET_KEY` | S3 | — | Secret key |
| `S3_URL` | S3 | — | S3-compatible HTTPS endpoint |
| `S3_EUROSTAT_BUCKET` | optional | `eurostat` | Bucket name |
| `S3_REGION` | optional | `fsn1` | Region for signing |
| `EUROSTAT_PARALLEL` | optional | `16` | Parallel fetch/upload workers |
| `CLICKHOUSE_HOST` | optional | `127.0.0.1` | ClickHouse host |
| `CLICKHOUSE_PORT` | optional | `9000` | Native port (`9440` with TLS) |
| `CLICKHOUSE_HTTP_PORT` | optional | `8123` | HTTP(S) port (`8443` with TLS) |
| `CLICKHOUSE_USER` | optional | `default` | Database user |
| `CLICKHOUSE_PASSWORD` | ClickHouse | — | Database password |
| `CLICKHOUSE_DATABASE` | optional | `eurostat` | Target database |
| `CLICKHOUSE_TLS` | remote CH | `0` | `1` = HTTPS / native TLS |
| `CLICKHOUSE_INGEST_PARALLEL` | optional | `4` | Parallel ingest workers |
| `EUROSTAT_DATA_DIR` | optional | `~/.local/share/eurostat` | Local data root (`/var/lib/eurostat` on servers) |
| `EUROSTAT_LOG_DIR` | optional | `/var/log/eurostat` | Sync/ingest/cron logs |
| `EUROSTAT_BIN` | optional | auto | CLI path for scripts |

Scripts load `~/.env` automatically (LF line endings).

## Scripts

| Script | Purpose |
|--------|---------|
| [`scripts/install.sh`](scripts/install.sh) | **Install** — curl \| bash or clone + build |
| [`scripts/install-cron.sh`](scripts/install-cron.sh) | Weekly cron — S3 sync + ClickHouse ingest |
| [`scripts/sync-s3.sh`](scripts/sync-s3.sh) | **Primary** — stream all datasets to S3 |
| [`scripts/clickhouse-ingest.sh`](scripts/clickhouse-ingest.sh) | S3 manifest → ClickHouse (`--resume`) |
| [`scripts/sync-all.sh`](scripts/sync-all.sh) | Local mirror to `~/.local/share/eurostat/datasets/` |
| [`scripts/sync-data.sh`](scripts/sync-data.sh) | Smoke test (cache + one fetch) |
| [`scripts/upload-public.sh`](scripts/upload-public.sh) | Refresh `public/codelists/` in repo |
| [`notebooks/`](notebooks/) | ClickHouse analysis notebooks (GDP, HICP, unemployment) |

## Data layout

**S3 (production):**

```
s3://eurostat/datasets/nama/nama_10_gdp.parquet.zstd
s3://eurostat/manifest/manifest.jsonl   # upload progress log
```

**Local (optional dev/cache):**

```
~/.local/share/eurostat/
├── eurostat.db      # catalogue metadata (~8966 datasets)
├── exports/         # single-dataset fetches
├── datasets/        # local mirror output
└── manifest.jsonl   # local mirror progress
```

On servers, set `EUROSTAT_DATA_DIR=/var/lib/eurostat` (see [docs/CONFIGURATION.md](docs/CONFIGURATION.md)).

**Repo (`public/`):** committed codelist catalogues — see [public/README.md](public/README.md).

## Usage

```bash
# Catalogue
eurostat cache refresh
eurostat search gdp
eurostat info nama_10_gdp

# Single dataset fetch
eurostat fetch nama_10_gdp --filter geo=DE --filter time=2020 --format parquet

# Bulk inventory
eurostat bulk list

# Local mirror
eurostat mirror run --source dissemination --resume --parallel 8
eurostat mirror status

# Direct S3 mirror (same as sync-s3.sh)
eurostat mirror s3 --source dissemination --resume --parallel 16
eurostat mirror status --s3

# ClickHouse ingest (see docs/DEPLOY.md)
eurostat clickhouse ingest --resume --parallel 4
eurostat clickhouse status
eurostat clickhouse load-codelists
```

## Troubleshooting

| Problem | Fix |
|---------|-----|
| S3 auth / 403 | Keys must match the target bucket; check endpoint in your provider console |
| `$'\r': command not found` | `sed -i 's/\r$//' ~/.env` |
| `S3_ACCESS_KEY … is not set` | Configure `~/.env` (see [docs/CONFIGURATION.md](docs/CONFIGURATION.md)) or run via `./scripts/sync-s3.sh` |
| `search` returns nothing | `eurostat cache refresh` |
| No local datasets | Use `sync-s3.sh` for S3, or `sync-all.sh` for local mirror |

Official Eurostat API docs: [API data access](https://ec.europa.eu/eurostat/web/user-guides/data-browser/api-data-access/api-introduction)

## Development

```bash
make check          # fast typecheck
make test           # workspace tests
cargo test -p eurostat --features s3
```

## Faster compilations

| Technique | Command | Effect |
|-----------|---------|--------|
| Check, don't build | `cargo check -p eurostat-cli` | Skips linking |
| Minimal features | `cargo check -p eurostat --no-default-features` | Skips arrow/sqlx |
| mold linker | `.cargo/config.toml` | Faster linking on Linux |

## Workspace layout

```
crates/
  eurostat/       # library (mirror, s3, export, cache)
  eurostat-cli/   # CLI binary
docs/
  CONFIGURATION.md # env vars, S3, ClickHouse, TLS
  SERVER.md        # server install & first S3 sync
  DEPLOY.md        # S3 → ClickHouse ops, cron, monitoring
  CLICKHOUSE.md    # schema design & CLI reference
  ARCHITECTURE.md  # module map & data flow
examples/         # runnable CLI scripts
public/           # codelist reference data
scripts/          # sync-s3.sh, clickhouse-ingest.sh, sync-all.sh
```

## MSRV

Rust 1.85+

## License

MIT — see [LICENSE](LICENSE).
