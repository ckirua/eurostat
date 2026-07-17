#!/usr/bin/env bash
# Sync all Eurostat data: catalogue cache + full dissemination mirror.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DATA_DIR="${EUROSTAT_DATA_DIR:-$HOME/.local/share/eurostat}"
PARALLEL="${EUROSTAT_PARALLEL:-8}"

# Prefer installed binary; fall back to cargo run from this repo.
if [[ -n "${EUROSTAT_BIN:-}" ]]; then
  BIN=("$EUROSTAT_BIN")
elif command -v eurostat >/dev/null 2>&1; then
  BIN=(eurostat)
else
  BIN=(cargo run --quiet -p eurostat-cli --)
fi

run() {
  "${BIN[@]}" "$@"
}

echo "==> Data directory: $DATA_DIR"
echo "==> Command: ${BIN[*]}"

echo "==> Refreshing catalogue cache"
run cache refresh

echo "==> Mirroring dissemination datasets to parquet.zstd (resume, parallel=$PARALLEL)"
run mirror run --source dissemination --resume --parallel "$PARALLEL"

echo "==> Mirror status"
run mirror status

echo "==> Datasets on disk: $(find "$DATA_DIR/datasets" -name '*.parquet.zstd' 2>/dev/null | wc -l) file(s)"
