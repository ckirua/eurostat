# Configuration

How to configure the Eurostat CLI and scripts for local use, S3 mirroring, and ClickHouse analytics.

All secrets live in **`~/.env`** (never commit this file). Scripts under `scripts/` load it automatically. A ready-to-edit template ships as [`.env.example`](../.env.example).

Add the variables you need from `.env.example` into `~/.env` (append or edit in place). Never replace `~/.env` with the example file.

Use **LF** line endings (not Windows CRLF). If you see `$'\r': command not found`:

```bash
sed -i 's/\r$//' ~/.env
```

---

## What do you need?

| You want to… | Variables to set |
|--------------|------------------|
| Browse / fetch / search Eurostat data | None required (optional: `EUROSTAT_DATA_DIR`) |
| Mirror datasets to object storage | `S3_EUROSTAT_*` |
| Ingest S3 data into ClickHouse | `S3_EUROSTAT_*` + `CLICKHOUSE_*` |

---

## Object storage (`S3_EUROSTAT_*`)

Works with any **S3-compatible** endpoint (AWS S3, MinIO, Hetzner Object Storage, Cloudflare R2, …).

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `S3_EUROSTAT_ACCESS_KEY` | yes (for S3) | — | Access key id |
| `S3_EUROSTAT_SECRET_KEY` | yes (for S3) | — | Secret access key |
| `S3_EUROSTAT_ENDPOINT` | yes (for S3) | — | HTTPS endpoint, e.g. `https://fsn1.your-objectstorage.com` |
| `S3_EUROSTAT_BUCKET` | no | `eurostat` | Bucket name |
| `S3_EUROSTAT_REGION` | no | `fsn1` | Region label used for request signing |
| `EUROSTAT_PARALLEL` | no | `16` | Parallel fetch / upload workers |

Example (Hetzner Object Storage):

```bash
S3_EUROSTAT_BUCKET=eurostat
S3_EUROSTAT_ENDPOINT=https://fsn1.your-objectstorage.com
S3_EUROSTAT_REGION=fsn1
S3_EUROSTAT_ACCESS_KEY=your_access_key
S3_EUROSTAT_SECRET_KEY=your_secret_key
EUROSTAT_PARALLEL=16
```

Create keys in your provider console for the bucket you intend to use. Path-style access is supported.

---

## ClickHouse (`CLICKHOUSE_*`)

Optional. Needed only for the analytics pipeline (`eurostat clickhouse …` / `scripts/clickhouse-ingest.sh`).

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CLICKHOUSE_PASSWORD` | yes (for ClickHouse) | — | Database password (may be empty for a local default user) |
| `CLICKHOUSE_HOST` | no | `127.0.0.1` | Hostname or IP |
| `CLICKHOUSE_PORT` | no | `9000` | Native protocol port (`9440` is typical with TLS) |
| `CLICKHOUSE_HTTP_PORT` | no | `8123` | HTTP(S) port used by the Rust ingest client (`8443` is typical with TLS) |
| `CLICKHOUSE_USER` | no | `default` | Database user |
| `CLICKHOUSE_DATABASE` | no | `eurostat` | Target database |
| `CLICKHOUSE_TLS` | no | `0` | `1` / `true` / `yes` → HTTPS + native TLS (`--secure`) |
| `CLICKHOUSE_INGEST_PARALLEL` | no | `4` | Parallel ingest workers |

### Local ClickHouse (default)

HTTP on localhost is fine — credentials never leave the machine:

```bash
CLICKHOUSE_HOST=127.0.0.1
CLICKHOUSE_PORT=9000
CLICKHOUSE_HTTP_PORT=8123
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=
CLICKHOUSE_DATABASE=eurostat
CLICKHOUSE_TLS=0
CLICKHOUSE_INGEST_PARALLEL=4
```

### Remote ClickHouse (TLS required)

Set `CLICKHOUSE_TLS=1` so the client uses **HTTPS** and the native client uses **`--secure`**. Without TLS, passwords would be sent in cleartext over the network.

```bash
CLICKHOUSE_HOST=clickhouse.example.com
CLICKHOUSE_PORT=9440
CLICKHOUSE_HTTP_PORT=8443
CLICKHOUSE_USER=eurostat
CLICKHOUSE_PASSWORD=your_password
CLICKHOUSE_DATABASE=eurostat
CLICKHOUSE_TLS=1
CLICKHOUSE_INGEST_PARALLEL=4
```

Ingest scripts never pass the password on the process command line. They load `CLICKHOUSE_PASSWORD` from the environment via a short-lived client config file.

---

## Paths and tooling

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `EUROSTAT_DATA_DIR` | no | `~/.local/share/eurostat` | Local data root (SQLite cache, exports, local mirror) |
| `EUROSTAT_LOG_DIR` | no | `/var/log/eurostat` | Directory for sync/ingest/cron logs |
| `EUROSTAT_BIN` | no | `eurostat` or `cargo run` | Path to the CLI binary for shell scripts |
| `ENV_FILE` | no | `~/.env` | Alternate env file path (scripts only) |

### Data directory

Default follows the XDG Base Directory spec on Linux/macOS:

```
~/.local/share/eurostat/
├── eurostat.db
├── config.toml      # optional
├── exports/
├── bulk/
├── datasets/
└── manifest.jsonl
```

**Server installs** should use a system path owned by the service user:

```bash
export EUROSTAT_DATA_DIR=/var/lib/eurostat
# create once:
sudo mkdir -p /var/lib/eurostat
sudo chown "$(id -u):$(id -g)" /var/lib/eurostat
```

Legacy installs that still use `~/.data/eurostat` can keep working by setting `EUROSTAT_DATA_DIR` explicitly.

### Logs

Operational logs (cron and background jobs) go under **`/var/log/eurostat/`** by default — not the git checkout:

| File | Produced by |
|------|-------------|
| `/var/log/eurostat/sync-s3.log` | `./scripts/sync-s3.sh` (cron / `nohup`) |
| `/var/log/eurostat/clickhouse-ingest.log` | `./scripts/clickhouse-ingest.sh` (cron / `nohup`) |

`./scripts/install.sh` and `./scripts/install-cron.sh` create the directory (using `sudo` if needed) and own it to the installing user. Override the path with `EUROSTAT_LOG_DIR` if `/var/log` is not appropriate on your host.

```bash
# Follow a running job
tail -f /var/log/eurostat/sync-s3.log
tail -f /var/log/eurostat/clickhouse-ingest.log
```

---

## Security checklist

- Keep secrets in `~/.env` with mode `600` (`chmod 600 ~/.env`).
- Never commit `.env`, keys, or PEM files — they are gitignored.
- Use `.env.example` as the public template (empty placeholders only).
- Enable `CLICKHOUSE_TLS=1` whenever ClickHouse is not on loopback.
- Prefer running sync / ingest via `./scripts/*.sh` so env loading is consistent.

---

## Related guides

| Guide | When to read it |
|-------|-----------------|
| [SERVER.md](SERVER.md) | Fresh server install and first S3 sync |
| [DEPLOY.md](DEPLOY.md) | S3 → ClickHouse ops, cron, monitoring |
| [CLICKHOUSE.md](CLICKHOUSE.md) | Schema design and ClickHouse CLI reference |
| [README.md](../README.md) | Project overview and quick start |
