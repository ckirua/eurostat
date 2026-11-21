# Running on a small box — adaptation notes (2026-11-20)

Findings from adapting the full pipeline (S3 mirror + ClickHouse) to a 7.1 GB RAM / 4-core
Ubuntu box that already runs Postgres, ClickHouse, Grafana and dev daemons.

## Repo/toolchain adaptations (done)

| Item | Problem | Fix |
|------|---------|-----|
| Toolchain | Committed `Cargo.lock` needs rustc ≥ 1.89 (`clickhouse 0.15`, `icu 2.x`, `idna_adapter 1.2.2`, `time-macros 0.2.31`); box default was pinned to 1.85 — the lockfile actually violates the workspace MSRV (CI uses newer stable) | `rustup toolchain install stable --profile minimal` (1.100) + `rustup override set stable --path <repo>` — no repo change, no `Cargo.lock` churn |
| Linker | `.cargo/config.toml` forces `-fuse-ld=mold` | `apt-get install mold` |

`~/.env` additions (this box): `S3_EUROSTAT_*` (legacy aliases) + `CLICKHOUSE_*` block.
The **legacy `S3_EUROSTAT_*` aliases matter**: the shared `S3_*` key in `~/.env` has no
access to the `eurostat` bucket, so eurostat needs its own key pair without touching the
shared variables used by other apps.

## Credential finding

The bucket `eurostat` is owned by the key with access id `EI9DSN…` (the `MERCAT_S3_*`
pair). Its secret had been **rotated**: `~/.env` carried the old secret (signature
failures started mid-session), the current one was in a stale `~/.env` backup on the box.
Lesson: when S3 auth stops working with a key that worked minutes ago, check for rotated
secrets before doubting the code or the clock.

## Memory ceiling — the hard limit on this box

The S3 mirror converts each bulk file **fully in memory**: raw bytes → decompressed TSV →
`Vec<String>` table → Arrow → ZSTD parquet. The corpus has datasets that dominate:

- `AVIA_GOEXAC`: **22.7 MB gzipped → 844 MB TSV** (547k rows, 37:1 expansion)
- Observed peak for that one file: **~6 GB RAM + 6 GB swap (12 GB total)**
- With `--parallel 2` two such conversions overlap → OOM at ~5.8 GB RSS on a 7.1 GB box

Five full-corpus runs (`--parallel` 8 / 4 / 2 / 1 / 1) were OOM-killed, always in the
`AVIA_GO*` region (first large files after the `AACT`–`AVIA` skip-streak). Resume works as
designed — each restart re-scans A→Z with HEAD requests, re-verifies unchanged objects, and
reaches the same region within minutes.

Conclusion: **a ≥ 16 GB RAM box is required for the full dissemination mirror** (or a
streaming/chunked conversion, which would cut per-file memory to ~1–2 GB — not implemented).

Swap added on this box while investigating (kept, harmless): `/swap.img` 4 GB +
`/swapfile2` 4 GB + `/swapfile3` 8 GB, all in `/etc/fstab`. Swap alone is not enough:
the 12 GB working set exceeds RAM + available swap.

## What was verified before the memory wall

- `mirror s3 --resume` skip logic (inventory change stamp vs S3 `Last-Modified`) — correct
- ~25 datasets re-uploaded (`AACT`–`AVIA` stretch), rest correctly skipped
- `mirror status --s3` against the live manifest — works after the foreign-line fix
  (see PR #7 / issue #6)
- Bucket state: `eurostat` holds ~7,135 `status=ok` datasets + `manifest/manifest.jsonl`

## Resume on a big box

1. Clone repo; `rustup override set stable` (or newer) + `mold`.
2. `~/.env`: `S3_EUROSTAT_BUCKET=eurostat`, `S3_EUROSTAT_ENDPOINT=https://fsn1.<your-objectstorage>.com`,
   `S3_EUROSTAT_REGION=fsn1`, `S3_EUROSTAT_ACCESS_KEY` / `S3_EUROSTAT_SECRET_KEY` (the key
   that can read/write the `eurostat` bucket).
3. `EUROSTAT_PARALLEL=16 ./scripts/sync-s3.sh` — resumes; skips everything unchanged.
4. ClickHouse (optional): `CLICKHOUSE_*` in `~/.env`, apply `sql/clickhouse/00{1,2,3}_schema.sql`,
   `eurostat cache refresh`, `eurostat clickhouse load-codelists`, `./scripts/clickhouse-ingest.sh`.
