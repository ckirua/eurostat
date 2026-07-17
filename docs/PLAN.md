# Eurostat Rust CLI & Library

## Vision

Build a production-grade Rust CLI and library for Eurostat/Comext.

## Tech

- Rust stable (MSRV 1.85)
- tokio, reqwest
- clap
- serde, serde_json
- quick-xml
- sqlx (SQLite) — feature `cache`
- arrow + parquet — feature `export`
- polars — optional feature `polars-export`
- indicatif — feature `bulk`
- tracing
- flate2, thiserror, anyhow, directories

## API surface

| API | Base path |
|-----|-----------|
| Statistics | `/statistics/1.0/data/{code}` |
| SDMX 2.1 | `/sdmx/2.1/data/{flow}/{key}` |
| SDMX 3.0 | `/sdmx/3.0/data/dataflow/ESTAT/{code}/1.0/{key}` |
| Catalogue | `/catalogue/toc/txt?lang=en` |
| Async | `/1.0/async/status/{uuid}`, `/1.0/async/data/{uuid}` |
| Bulk inventory | `/files/inventory?type=data` |
| Comext | `https://ec.europa.eu/eurostat/api/comext/dissemination` |

## Milestones

### Phase 0 — Foundation
- Cargo workspace (`eurostat`, `eurostat-cli`, `eurostat-bench`)
- CI, fmt/clippy, release profile
- mold linker + dev profile optimizations
- Feature-gated heavy deps (`cache`, `export`, `bulk`)

### Phase 1 — Core
- HTTP client, config, errors, logging, CLI skeleton

### Phase 2 — Catalogue
- TOC parser, SQLite cache, FTS search, fuzzy fallback

### Phase 3 — Fetch
- Statistics, SDMX 2.1/3.0, dimensions, async jobs, Comext routing

### Phase 4 — Storage
- CSV, JSON, Parquet export via Arrow

### Phase 5 — Bulk
- Inventory API + legacy HTML fallback, resume, parallel downloads

### Phase 6 — Polish
- Shell completions, benchmarks, docs, crates.io release

## Agent rules

- Keep modules under 500 LOC where practical
- Integration tests for public functionality
- No `unwrap()` outside tests
- Prefer streaming over buffering
- Document all public APIs

## Workspace structure

```
crates/
  eurostat/       # library
  eurostat-cli/   # binary
  eurostat-bench/ # benchmarks
```

## Stretch goals

- PyO3 bindings
- DuckDB export
- DataFusion integration
- TUI mode

## Release policy

No public release until Phase 6 is complete.
