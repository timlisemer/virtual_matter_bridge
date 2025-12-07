.PHONY: check build run run-debug run-trace test clean

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
