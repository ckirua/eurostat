#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
EUROSTAT="${EUROSTAT:-cargo run --quiet -p eurostat-cli --}"

echo "==> Listing bulk download files (first entries)"
$EUROSTAT bulk list
