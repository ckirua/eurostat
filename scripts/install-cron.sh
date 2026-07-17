#!/usr/bin/env bash
# Install or remove Eurostat weekly cron jobs (S3 sync + ClickHouse ingest).
#
# Usage:
#   ./scripts/install-cron.sh                  # install for this repo
#   ./scripts/install-cron.sh --dir ~/eurostat
#   ./scripts/install-cron.sh --uninstall
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="${EUROSTAT_INSTALL_DIR:-$ROOT}"
LOG_DIR="${EUROSTAT_LOG_DIR:-/var/log/eurostat}"
UNINSTALL=0
MARKER_BEGIN="# EUROSTAT-CRON-BEGIN (managed by scripts/install-cron.sh)"
MARKER_END="# EUROSTAT-CRON-END"

usage() {
  cat <<EOF
Install Eurostat weekly cron jobs

Usage:
  ./scripts/install-cron.sh [options]

Options:
  --dir PATH     Repo path for cron cd (default: script parent dir)
  --uninstall    Remove Eurostat cron block only
  -h, --help     Show this help

Schedule (UTC):
  Sunday 03:00  sync-s3.sh       → Eurostat API to S3
  Sunday 03:30  clickhouse-ingest.sh → S3 to ClickHouse

Logs (default): /var/log/eurostat/
  Override with EUROSTAT_LOG_DIR
Docs: docs/DEPLOY.md , docs/CONFIGURATION.md
EOF
}

log() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) REPO="${2:?}"; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done

REPO="$(cd "$REPO" && pwd)"

ensure_log_dir() {
  if [[ -d "$LOG_DIR" && -w "$LOG_DIR" ]]; then
    return 0
  fi
  if mkdir -p "$LOG_DIR" 2>/dev/null && [[ -w "$LOG_DIR" ]]; then
    log "Created log directory $LOG_DIR"
    return 0
  fi
  if command -v sudo >/dev/null 2>&1; then
    sudo mkdir -p "$LOG_DIR"
    sudo chown "$(id -u):$(id -g)" "$LOG_DIR"
    sudo chmod 755 "$LOG_DIR"
    log "Created log directory $LOG_DIR (via sudo)"
    return 0
  fi
  die "cannot create writable log directory $LOG_DIR (set EUROSTAT_LOG_DIR or run with sudo once)"
}

have_crontab() {
  crontab -l >/dev/null 2>&1
}

read_crontab() {
  if have_crontab; then
    crontab -l
  fi
}

strip_managed_block() {
  awk -v begin="$MARKER_BEGIN" -v end="$MARKER_END" '
    $0 == begin { skip=1; next }
    $0 == end { skip=0; next }
    !skip { print }
  '
}

strip_legacy_eurostat_lines() {
  grep -v 'scripts/sync-s3\.sh' \
    | grep -v 'scripts/clickhouse-ingest\.sh' \
    | grep -v 'Eurostat weekly pipeline' \
    | grep -v 'Sunday 03:00 UTC' \
    | grep -v 'Sunday 03:30 UTC' \
    || true
}

cron_block() {
  cat <<EOF
$MARKER_BEGIN
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:\$HOME/.cargo/bin
# Sunday 03:00 UTC — Eurostat → S3
0 3 * * 0 cd $REPO && ./scripts/sync-s3.sh >> $LOG_DIR/sync-s3.log 2>&1
# Sunday 03:30 UTC — S3 → ClickHouse
30 3 * * 0 cd $REPO && ./scripts/clickhouse-ingest.sh >> $LOG_DIR/clickhouse-ingest.log 2>&1
$MARKER_END
EOF
}

already_installed() {
  local current
  current="$(read_crontab || true)"
  [[ -n "$current" ]] || return 1
  printf '%s\n' "$current" | grep -Fq "$MARKER_BEGIN" || return 1
  printf '%s\n' "$current" | grep -Fq "cd $REPO && ./scripts/sync-s3.sh" || return 1
  printf '%s\n' "$current" | grep -Fq "$LOG_DIR/sync-s3.log" || return 1
}

install_cron() {
  ensure_log_dir

  if already_installed; then
    log "Cron already installed for $REPO (unchanged)"
    log "Logs: $LOG_DIR/"
    return 0
  fi

  local filtered new_crontab
  filtered="$(read_crontab | strip_managed_block | strip_legacy_eurostat_lines | sed '/^[[:space:]]*$/d' || true)"

  if [[ -n "$filtered" ]]; then
    new_crontab="$(printf '%s\n\n' "$filtered"; cron_block)"
  else
    new_crontab="$(cron_block)"
  fi

  printf '%s\n' "$new_crontab" | crontab -
  log "Installed weekly cron for $REPO"
  log "  Sunday 03:00 UTC — ./scripts/sync-s3.sh"
  log "  Sunday 03:30 UTC — ./scripts/clickhouse-ingest.sh"
  log "  Logs: $LOG_DIR/sync-s3.log , $LOG_DIR/clickhouse-ingest.log"
  log "Verify: crontab -l"
}

uninstall_cron() {
  if ! have_crontab; then
    log "No crontab — nothing to remove"
    return 0
  fi

  local current filtered
  current="$(read_crontab)"
  if ! printf '%s\n' "$current" | grep -Fq "$MARKER_BEGIN"; then
    log "No Eurostat cron block found"
    return 0
  fi

  filtered="$(printf '%s\n' "$current" | strip_managed_block | sed '/^[[:space:]]*$/d' || true)"
  if [[ -n "$filtered" ]]; then
    printf '%s\n' "$filtered" | crontab -
  else
    crontab -r 2>/dev/null || true
  fi
  log "Removed Eurostat cron block"
}

if [[ "$UNINSTALL" -eq 1 ]]; then
  uninstall_cron
else
  install_cron
fi
