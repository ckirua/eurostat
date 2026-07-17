.PHONY: check test fmt clippy doc bench fast install release sync-s3

# Install CLI to ~/.cargo/bin/eurostat
install:
	cargo install --path crates/eurostat-cli --force

# Fastest iteration: typecheck only, no linking
check:
	cargo check -p eurostat-cli

# Even faster: lib only, no heavy features
fast:
	cargo check -p eurostat --no-default-features

test:
	cargo test --workspace

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

doc:
	cargo doc --workspace --no-deps

bench:
	cargo bench -p eurostat-bench

release:
	cargo build --release -p eurostat-cli

# Full Eurostat → S3 sync (loads ~/.env)
sync-s3:
	./scripts/sync-s3.sh

# Full Eurostat → local mirror (cache + datasets under ~/.data/eurostat)
sync-local:
	./scripts/sync-all.sh
