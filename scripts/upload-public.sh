#!/usr/bin/env bash
# Fetch Eurostat codelist inventory into public/ and commit to the repo.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PUBLIC_DIR="$ROOT/public/codelists"
INVENTORY_URL="${EUROSTAT_CODELIST_INVENTORY_URL:-https://ec.europa.eu/eurostat/api/dissemination/files/inventory?type=codelist&lang=en}"
COMMIT_MSG="${UPLOAD_COMMIT_MSG:-Update public codelist inventory}"

mkdir -p "$PUBLIC_DIR"

echo "==> Fetching codelist inventory"
curl -fsSL "$INVENTORY_URL" -o "$PUBLIC_DIR/inventory.tsv"
LINES=$(wc -l < "$PUBLIC_DIR/inventory.tsv")
echo "    Wrote $PUBLIC_DIR/inventory.tsv ($LINES lines)"

if [[ ! -f "$PUBLIC_DIR/catalogue.tsv" ]]; then
  echo "WARNING: $PUBLIC_DIR/catalogue.tsv missing (UI search export); only inventory.tsv updated"
fi

git add public/

if git diff --staged --quiet; then
  echo "==> No changes to commit"
  exit 0
fi

git commit -m "$COMMIT_MSG"
echo "==> Committed public codelist updates"

if [[ "${PUSH:-}" == "1" ]]; then
  git push
  echo "==> Pushed to origin"
fi
