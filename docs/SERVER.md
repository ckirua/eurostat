# Server install & sync

Install the Eurostat CLI on a Linux server and stream dissemination datasets to S3-compatible object storage.

## Prerequisites

- Linux server with Git
- Rust 1.85+ — installed automatically by `install.sh` if missing
- `~/.env` with S3 credentials (see [CONFIGURATION.md](CONFIGURATION.md))

Optional: `p7zip-full` only if you later enable COMEXT `.7z` mirroring.

## 0. One-line install

```bash
curl -fsSL https://raw.githubusercontent.com/ckirua/eurostat/main/scripts/install.sh | bash
```

Defaults: clone to `~/eurostat`, branch `main`, release binary at `~/eurostat/target/release/eurostat`.

Options:

```bash
curl -fsSL .../install.sh | bash -s -- --dir /opt/eurostat --branch main --global
./scripts/install.sh --help
```

After install, add the needed keys from `.env.example` into `~/.env` (see [CONFIGURATION.md](CONFIGURATION.md)). Do not replace `~/.env`.

## 1. Clone and build

```bash
git clone https://github.com/ckirua/eurostat.git
cd eurostat
./scripts/install.sh --dir . --global

# Or manually:
# cargo install --path crates/eurostat-cli
# make release   # → target/release/eurostat
```

Verify:

```bash
eurostat --help
eurostat bulk list | head
```

## 2. Environment variables

Put secrets in `~/.env` (never commit this file). Use `.env.example` as a reference and add only the keys you need — never replace `~/.env` with the example.

Minimum for S3 sync:

```bash
S3_EUROSTAT_BUCKET=eurostat
S3_URL=https://fsn1.your-objectstorage.com
S3_REGION=fsn1
S3_ACCESS_KEY=your_access_key
S3_SECRET_KEY=your_secret_key
EUROSTAT_PARALLEL=16
```

Any S3-compatible provider works (AWS, MinIO, Hetzner Object Storage, R2, …). Create keys in your provider console for the target bucket. Path-style access is supported.

Full reference (ClickHouse, TLS, paths): **[CONFIGURATION.md](CONFIGURATION.md)**.

Fix CRLF if `source ~/.env` prints `$'\r': command not found`:

```bash
sed -i 's/\r$//' ~/.env
```

## 3. Full sync to S3 (recommended)

Fetches ~8k datasets from Eurostat, converts to parquet.zstd in memory, uploads directly — **no local `datasets/` directory required**.

```bash
cd eurostat
chmod +x scripts/sync-s3.sh
./scripts/sync-s3.sh
```

Safe to stop and re-run: `--resume` skips objects already in the bucket.

**ClickHouse pipeline** (S3 → analytics DB): [DEPLOY.md](DEPLOY.md)

### S3 layout

```
s3://eurostat/
├── datasets/
│   ├── nama/nama_10_gdp.parquet.zstd
│   ├── ei/ei_bssi_m_r2.parquet.zstd
│   └── aact/AACT_ALI01.parquet.zstd
└── codelists/          # optional: ./scripts/upload-public.sh
    ├── catalogue.tsv
    └── inventory.tsv
```

Prefix folder = first segment of dataset id (`nama` from `nama_10_gdp`).

## 4. Optional: local mirror

If you want a local copy under `~/.local/share/eurostat/datasets/` instead of (or before) S3:

```bash
./scripts/sync-all.sh
```

## 5. Catalogue cache only

For search/metadata without bulk data:

```bash
eurostat cache refresh
eurostat search gdp
```

SQLite cache: `~/.local/share/eurostat/eurostat.db` (~8,966 dataset listings).

## 6. Makefile shortcuts

```bash
make release    # build target/release/eurostat
make check      # fast typecheck
make test       # workspace tests
```

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `S3_ACCESS_KEY … is not set` | `source ~/.env` or run via `./scripts/sync-s3.sh` |
| `$'\r': command not found` | `sed -i 's/\r$//' ~/.env` |
| 403 / auth errors | Use keys created for the target bucket |
| Wrong endpoint | Check provider console; include `https://` and the correct region endpoint |
| Slow / interrupted | Re-run `./scripts/sync-s3.sh` — resume skips uploaded objects |
