#!/usr/bin/env bash
# Sync Eurostat data into ~/.local/share/eurostat and verify the layout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DATA_DIR="${EUROSTAT_DATA_DIR:-$HOME/.local/share/eurostat}"
EUROSTAT="${EUROSTAT:-cargo run --quiet -p eurostat-cli --}"

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

echo "==> Data directory: $DATA_DIR"

echo "==> Refreshing catalogue cache"
CACHE_OUT=$($EUROSTAT cache refresh 2>&1) || fail "cache refresh failed: $CACHE_OUT"
echo "$CACHE_OUT"

CACHED_COUNT=$(echo "$CACHE_OUT" | rg -o 'Cached [0-9]+ datasets' | rg -o '[0-9]+' || true)
if [[ -z "$CACHED_COUNT" || "$CACHED_COUNT" -lt 100 ]]; then
  fail "expected hundreds+ datasets in cache, got: ${CACHED_COUNT:-none}"
fi
echo "    Cached $CACHED_COUNT datasets"

echo "==> Searching for GDP datasets"
SEARCH_OUT=$($EUROSTAT search gdp --limit 3 2>&1) || fail "search failed: $SEARCH_OUT"
echo "$SEARCH_OUT"
if echo "$SEARCH_OUT" | rg -q 'No datasets matched'; then
  fail "search returned no GDP datasets"
fi

echo "==> Fetching nama_10_gdp as Parquet"
FETCH_OUT=$($EUROSTAT fetch nama_10_gdp --filter geo=DE --filter time=2020 --format parquet 2>&1) || fail "fetch failed: $FETCH_OUT"
echo "$FETCH_OUT"

DB_PATH="$DATA_DIR/eurostat.db"
PARQUET_PATH="$DATA_DIR/exports/nama_10_gdp.parquet"

[[ -f "$DB_PATH" ]] || fail "missing database: $DB_PATH"
[[ -f "$PARQUET_PATH" ]] || fail "missing parquet export: $PARQUET_PATH"

echo "==> Verification passed"
echo "    $DB_PATH ($(du -h "$DB_PATH" | cut -f1))"
echo "    $PARQUET_PATH ($(du -h "$PARQUET_PATH" | cut -f1))"
echo "    exports: $(ls -1 "$DATA_DIR/exports" | wc -l) file(s)"
