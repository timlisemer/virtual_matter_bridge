.PHONY: check build run run-debug run-trace test clean dev dev-reset commission status

check:
	cargo check
	cargo fmt
	cargo clippy -- -D warnings

build:
	cargo build --release

run:
	cargo run

# Run with debug logging (shows UDP packet flow)
run-debug:
	RUST_LOG=debug cargo run

# Run with trace logging (shows full packet dumps)
run-trace:
	RUST_LOG=trace cargo run

test:
	cargo test

clean:
	cargo clean

# Development workflow targets

# Run with auto-reset on schema change (normal dev mode)
dev:
	RUST_LOG=info cargo run

# Run with forced persistence reset (always re-commission)
dev-reset:
	DEV_AUTO_RESET=1 RUST_LOG=info cargo run

# Commission to python-matter-server (run after dev/dev-reset)
# Set MATTER_SERVER_URL to override default ws://localhost:5580/ws
commission:
	cargo run --bin dev-commission -- commission

# Remove a node from python-matter-server
# Usage: make remove NODE_ID=123
remove:
	cargo run --bin dev-commission -- remove $(NODE_ID)

# Get status of all commissioned nodes
status:
	cargo run --bin dev-commission -- status
