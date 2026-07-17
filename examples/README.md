# Eurostat CLI examples

All examples assume the CLI is built from the repo root:

```bash
cargo build -p eurostat-cli
export EUROSTAT="cargo run --quiet -p eurostat-cli --"
```

By default, data is stored under `~/.local/share/eurostat/` (XDG data home — not inside this repo):

```
~/.local/share/eurostat/
├── config.toml      # optional settings
├── eurostat.db      # SQLite catalogue cache
├── exports/         # fetched CSV, JSON, Parquet files
└── bulk/            # bulk downloads
```

On a server, prefer a system path:

```bash
export EUROSTAT_DATA_DIR=/var/lib/eurostat
```

Populate and verify everything in one step:

```bash
./scripts/sync-data.sh
```

Override the location:

```bash
export EUROSTAT_DATA_DIR="$HOME/.local/share/eurostat"
```

## Scripts

| Script | What it does |
|--------|----------------|
| [`01-cache-and-search.sh`](01-cache-and-search.sh) | Refresh catalogue cache and search datasets |
| [`02-fetch-parquet.sh`](02-fetch-parquet.sh) | Fetch GDP data and save Parquet to `exports/` |
| [`03-fetch-csv.sh`](03-fetch-csv.sh) | Fetch data as CSV to the default exports dir |
| [`04-fetch-json-stdout.sh`](04-fetch-json-stdout.sh) | Fetch JSON to stdout (no file written) |
| [`05-bulk-list.sh`](05-bulk-list.sh) | List available bulk download files |

Run any script from the repo root:

```bash
./examples/02-fetch-parquet.sh
```
