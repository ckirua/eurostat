#!/usr/bin/env bash
# Fetch Eurostat datasets and upload parquet.zstd directly to Hetzner S3 (no local mirror).
#
# Server setup, credentials, and troubleshooting: docs/SERVER.md
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ENV_FILE="${ENV_FILE:-$HOME/.env}"
if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source <(sed 's/\r$//' "$ENV_FILE")
  set +a
fi

PARALLEL="${EUROSTAT_PARALLEL:-16}"

if [[ -n "${EUROSTAT_BIN:-}" ]]; then
  BIN=("$EUROSTAT_BIN")
elif [[ -x "$ROOT/target/release/eurostat" ]]; then
  BIN=("$ROOT/target/release/eurostat")
elif command -v eurostat >/dev/null 2>&1; then
  BIN=(eurostat)
else
  BIN=(cargo run --quiet -p eurostat-cli --)
fi

echo "==> S3 bucket: ${S3_EUROSTAT_BUCKET:-eurostat}"
echo "==> Endpoint: ${S3_EUROSTAT_ENDPOINT:-${S3_URL:-unset}}"
echo "==> Parallel: ${PARALLEL}"
echo "==> Command: ${BIN[*]} mirror s3 --source dissemination --resume"

exec "${BIN[@]}" mirror s3 --source dissemination --resume --parallel "$PARALLEL"
