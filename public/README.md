# Public data

Static reference files published in the repo (not runtime data under `~/.local/share/eurostat`).

## Codelists

| File | Source | Description |
|------|--------|-------------|
| [`codelists/catalogue.tsv`](codelists/catalogue.tsv) | Eurostat bulk UI search export | Code list index: code, label, last update, standard flag |
| [`codelists/inventory.tsv`](codelists/inventory.tsv) | `files/inventory?type=codelist` API | Full inventory with download URLs |

Refresh and commit with:

```bash
./scripts/upload-public.sh
PUSH=1 ./scripts/upload-public.sh   # also push to origin
```
