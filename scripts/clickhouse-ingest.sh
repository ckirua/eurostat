#!/usr/bin/env bash
# Ingest Eurostat parquet.zstd datasets from S3 into ClickHouse.
#
# Prerequisites and ops: docs/DEPLOY.md
#   - S3 mirror (./scripts/sync-s3.sh)
#   - ClickHouse schema (sql/clickhouse/002_schema.sql)
#   - ~/.env with S3_EUROSTAT_BUCKET / S3_URL / S3_ACCESS_KEY / S3_SECRET_KEY and CLICKHOUSE_*
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

CH_HOST="${CLICKHOUSE_HOST:-127.0.0.1}"
CH_PORT="${CLICKHOUSE_PORT:-9000}"
CH_USER="${CLICKHOUSE_USER:-default}"
CH_DB="${CLICKHOUSE_DATABASE:-eurostat}"

echo "==> ClickHouse: ${CH_USER}@${CH_HOST}:${CH_PORT}/${CH_DB}"
echo "==> S3 bucket: ${S3_EUROSTAT_BUCKET:-eurostat}"
echo "==> Endpoint: ${S3_EUROSTAT_ENDPOINT:-${S3_URL:-unset}}"

# Never put CLICKHOUSE_PASSWORD on argv (visible in ps /proc). Prefer a
# mode-600 client config that reads the password from the environment.
CH_CLIENT_CFG=""
cleanup_ch_client_cfg() {
  [[ -n "${CH_CLIENT_CFG}" ]] && rm -f "${CH_CLIENT_CFG}"
}
trap cleanup_ch_client_cfg EXIT

CH_CLIENT_ARGS=(--host "$CH_HOST" --port "$CH_PORT" --user "$CH_USER")
case "${CLICKHOUSE_TLS:-0}" in
  1|true|TRUE|yes|YES|on|ON) CH_CLIENT_ARGS+=(--secure) ;;
esac
if [[ -n "${CLICKHOUSE_PASSWORD:-}" ]]; then
  export CLICKHOUSE_PASSWORD
  CH_CLIENT_CFG="$(mktemp)"
  chmod 600 "$CH_CLIENT_CFG"
  printf '%s\n' '<config><password from_env="CLICKHOUSE_PASSWORD"/></config>' >"$CH_CLIENT_CFG"
  CH_CLIENT_ARGS+=(--config-file "$CH_CLIENT_CFG")
fi

# Connectivity checks
clickhouse-client "${CH_CLIENT_ARGS[@]}" -q "SELECT version() AS clickhouse_version"

clickhouse-client "${CH_CLIENT_ARGS[@]}" -q "EXISTS TABLE ${CH_DB}.observations" \
  | grep -q '^1$' || {
    echo "ERROR: schema not applied. Run:"
    echo "  clickhouse-client --multiquery < sql/clickhouse/002_schema.sql"
    exit 1
  }

if [[ -n "${EUROSTAT_BIN:-}" ]]; then
  BIN=("$EUROSTAT_BIN")
elif [[ -x "$ROOT/target/release/eurostat" ]]; then
  BIN=("$ROOT/target/release/eurostat")
elif command -v eurostat >/dev/null 2>&1; then
  BIN=(eurostat)
else
  BIN=(cargo run --quiet -p eurostat-cli --)
fi

# Full ingest from S3 manifest (resume skips unchanged datasets).
exec "${BIN[@]}" clickhouse ingest --resume --parallel "${CLICKHOUSE_INGEST_PARALLEL:-4}"
