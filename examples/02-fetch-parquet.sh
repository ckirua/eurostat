#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
EUROSTAT="${EUROSTAT:-cargo run --quiet -p eurostat-cli --}"

echo "==> Fetching nama_10_gdp (DE, 2020) as Parquet"
echo "    Default output: ~/.local/share/eurostat/exports/nama_10_gdp.parquet"
$EUROSTAT fetch nama_10_gdp --filter geo=DE --filter time=2020 --format parquet
