#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
EUROSTAT="${EUROSTAT:-cargo run --quiet -p eurostat-cli --}"

echo "==> Refreshing catalogue cache"
$EUROSTAT cache refresh

echo "==> Searching for GDP datasets"
$EUROSTAT search gdp --limit 5
