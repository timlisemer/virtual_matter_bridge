.PHONY: check run remove test status

# Linting and code quality checks
check:
	cargo check
	cargo fmt
	cargo clippy -- -D warnings

# Run the Matter bridge
# First run: Also run 'cargo run --bin dev-commission -- commission' in another terminal
# Subsequent runs: Bridge auto-reconnects to python-matter-server
run:
	RUST_LOG=info cargo run --bin virtual-matter-bridge

# Remove a commissioned node from python-matter-server
# Usage: make remove NODE_ID=123
remove:
	cargo run --bin dev-commission -- remove $(NODE_ID)

# Run all tests
test:
	cargo test

# Show commissioned nodes in python-matter-server
status:
	cargo run --bin dev-commission -- status
