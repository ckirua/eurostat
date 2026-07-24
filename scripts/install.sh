#!/usr/bin/env bash
# Install the Eurostat CLI on Linux or macOS.
#
# One-line install (public repo):
#   curl -fsSL https://raw.githubusercontent.com/ckirua/eurostat/main/scripts/install.sh | bash
#
# Options (after pipe, pass to bash -s):
#   curl -fsSL .../install.sh | bash -s -- --dir ~/eurostat --branch main
#
# Environment overrides:
#   EUROSTAT_REPO, EUROSTAT_BRANCH, EUROSTAT_INSTALL_DIR, EUROSTAT_SKIP_DEPS=1
set -euo pipefail

DEFAULT_REPO="${EUROSTAT_REPO:-https://github.com/ckirua/eurostat.git}"
DEFAULT_BRANCH="${EUROSTAT_BRANCH:-main}"
DEFAULT_DIR="${EUROSTAT_INSTALL_DIR:-$HOME/eurostat}"
INSTALL_GLOBAL=0
INSTALL_CRON=0
SKIP_DEPS=0
UPDATE_ONLY=0
INSTALL_DIR="$DEFAULT_DIR"
REPO="$DEFAULT_REPO"
BRANCH="$DEFAULT_BRANCH"

usage() {
  cat <<'EOF'
Eurostat CLI installer

Usage:
  curl -fsSL https://raw.githubusercontent.com/ckirua/eurostat/main/scripts/install.sh | bash
  ./scripts/install.sh [options]

Options:
  --dir PATH       Clone/build directory (default: ~/eurostat)
  --repo URL       Git remote (default: github.com/ckirua/eurostat)
  --branch NAME    Git branch (default: main)
  --global         Also install `eurostat` into ~/.cargo/bin
  --cron           Install weekly S3 + ClickHouse cron (scripts/install-cron.sh)
  --update         Pull latest and rebuild (existing clone only)
  --no-deps        Skip apt/brew dependency install
  -h, --help       Show this help

After install:
  1. Add keys from .env.example into ~/.env (never replace ~/.env)
  2. See docs/CONFIGURATION.md
  3. ./scripts/sync-s3.sh  (and docs/DEPLOY.md for ClickHouse)
EOF
}

log() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) INSTALL_DIR="${2:?}"; shift 2 ;;
    --repo) REPO="${2:?}"; shift 2 ;;
    --branch) BRANCH="${2:?}"; shift 2 ;;
    --global) INSTALL_GLOBAL=1; shift ;;
    --cron) INSTALL_CRON=1; shift ;;
    --update) UPDATE_ONLY=1; shift ;;
    --no-deps) SKIP_DEPS=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done

OS="$(uname -s)"
case "$OS" in
  Linux|Darwin) ;;
  *) die "unsupported OS: $OS (Linux and macOS only)" ;;
esac

have() { command -v "$1" >/dev/null 2>&1; }

install_system_deps() {
  [[ "$SKIP_DEPS" -eq 1 || "${EUROSTAT_SKIP_DEPS:-}" == "1" ]] && return 0

  if [[ "$OS" == "Linux" ]] && have apt-get; then
    if have sudo && sudo -n true 2>/dev/null; then
      log "Installing build dependencies (apt)"
      sudo apt-get update -qq
      sudo apt-get install -y -qq git curl build-essential pkg-config libssl-dev ca-certificates
    else
      warn "run as root or with passwordless sudo to auto-install: git curl build-essential pkg-config libssl-dev"
    fi
  elif [[ "$OS" == "Darwin" ]] && have brew; then
    log "Installing build dependencies (brew)"
    brew install git curl pkg-config openssl@3 2>/dev/null || true
  fi
}

install_rust() {
  if have cargo && have rustc; then
    log "Rust already installed: $(rustc --version)"
    return 0
  fi

  log "Installing Rust via rustup (MSRV 1.85+)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1091
  source "${HOME}/.cargo/env"
  have cargo || die "cargo not found after rustup install; add ~/.cargo/bin to PATH"
}

ensure_rust_msrv() {
  local msrv="1.85.0"
  if ! rustc --version | grep -qE '1\.(8[5-9]|[9][0-9])|[2-9][0-9]\.'; then
    log "Updating Rust toolchain (need >= $msrv)"
    rustup update stable
  fi
}

clone_or_update() {
  if [[ -d "$INSTALL_DIR/.git" ]]; then
    log "Updating existing clone: $INSTALL_DIR"
    git -C "$INSTALL_DIR" fetch origin "$BRANCH"
    git -C "$INSTALL_DIR" checkout "$BRANCH"
    git -C "$INSTALL_DIR" pull --ff-only origin "$BRANCH" || true
  elif [[ -d "$INSTALL_DIR" ]]; then
    die "$INSTALL_DIR exists but is not a git repo; remove it or pick --dir"
  else
    log "Cloning $REPO (branch $BRANCH) → $INSTALL_DIR"
    git clone --depth 1 --branch "$BRANCH" "$REPO" "$INSTALL_DIR"
  fi
}

build_cli() {
  log "Building release binary (S3 + ClickHouse features)"
  cd "$INSTALL_DIR"
  cargo build --release -p eurostat-cli
  chmod +x "$INSTALL_DIR/target/release/eurostat"

  if [[ "$INSTALL_GLOBAL" -eq 1 ]]; then
    log "Installing eurostat into ~/.cargo/bin"
    cargo install --path crates/eurostat-cli --force
  fi
}

setup_env_template() {
  local example="$INSTALL_DIR/.env.example"
  local env_file="${ENV_FILE:-$HOME/.env}"

  if [[ -f "$env_file" ]]; then
    log "Keeping existing $env_file"
    return 0
  fi

  if [[ -f "$example" ]]; then
    cp "$example" "$env_file"
    chmod 600 "$env_file"
    log "Created $env_file from .env.example — edit credentials before sync"
  fi
}

ensure_log_dir() {
  local log_dir="${EUROSTAT_LOG_DIR:-/var/log/eurostat}"
  if [[ -d "$log_dir" && -w "$log_dir" ]]; then
    return 0
  fi
  if mkdir -p "$log_dir" 2>/dev/null && [[ -w "$log_dir" ]]; then
    log "Created log directory $log_dir"
    return 0
  fi
  if have sudo; then
    sudo mkdir -p "$log_dir"
    sudo chown "$(id -u):$(id -g)" "$log_dir"
    sudo chmod 755 "$log_dir"
    log "Created log directory $log_dir (via sudo)"
    return 0
  fi
  warn "could not create $log_dir — create it before cron/background jobs (or set EUROSTAT_LOG_DIR)"
}

print_summary() {
  local bin="$INSTALL_DIR/target/release/eurostat"
  cat <<EOF

Eurostat CLI installed.

  Directory:  $INSTALL_DIR
  Binary:     $bin
  Version:    $($bin --version 2>/dev/null || echo 'run: $bin --help')

Quick test:
  $bin --help
  $bin bulk list | head

Environment (~/.env) — see docs/CONFIGURATION.md:
  S3_EUROSTAT_*       required for object-storage sync
  CLICKHOUSE_*        optional, for analytics ingest
  CLICKHOUSE_TLS=1    required when ClickHouse is remote

Logs (default): /var/log/eurostat/
  Override with EUROSTAT_LOG_DIR

Next steps:
  cd $INSTALL_DIR
  # edit ~/.env
  ./scripts/sync-s3.sh
  ./scripts/clickhouse-ingest.sh   # after ClickHouse schema + S3 data

Docs: $INSTALL_DIR/docs/CONFIGURATION.md
      $INSTALL_DIR/docs/SERVER.md
      $INSTALL_DIR/docs/DEPLOY.md

Optional — add to ~/.bashrc or ~/.zshrc:
  export EUROSTAT_BIN="$bin"
  export PATH="\$HOME/.cargo/bin:\$PATH"

Weekly cron (after ~/.env is configured):
  $INSTALL_DIR/scripts/install-cron.sh --dir $INSTALL_DIR
EOF
}

main() {
  install_system_deps
  install_rust
  # shellcheck disable=SC1091
  [[ -f "${HOME}/.cargo/env" ]] && source "${HOME}/.cargo/env"
  ensure_rust_msrv

  if [[ "$UPDATE_ONLY" -eq 1 ]]; then
    [[ -d "$INSTALL_DIR/.git" ]] || die "--update requires existing clone at $INSTALL_DIR"
    git -C "$INSTALL_DIR" pull --ff-only || true
  else
    clone_or_update
  fi

  build_cli
  setup_env_template
  ensure_log_dir
  if [[ "$INSTALL_CRON" -eq 1 ]]; then
    log "Installing weekly cron"
    "$INSTALL_DIR/scripts/install-cron.sh" --dir "$INSTALL_DIR"
  fi
  print_summary
}

main
