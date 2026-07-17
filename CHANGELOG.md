# Changelog

## 0.1.0 — 2026-07-11

### Added

- `eurostat` library crate with Statistics, SDMX 2.1/3.0, Catalogue, Comext, Async, and Bulk clients
- `eurostat-cli` binary with `search`, `info`, `dimensions`, `fetch`, `cache`, `bulk`, and `completions` commands
- SQLite FTS5 catalogue cache with optional fuzzy search
- CSV, JSON, and Parquet export via Arrow
- Integration tests with wiremock fixtures
- CI workflow with mold linker and rust-cache
- Compile-time optimizations: feature-gated deps, dev profile tuning, mold linker

### Documentation

- Updated README with usage and compile-speed tips
