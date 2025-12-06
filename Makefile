.PHONY: check build run test clean

check:
	cargo check
	cargo fmt
	cargo clippy -- -D warnings

build:
	cargo build --release

run:
	cargo run

test:
	cargo test

clean:
	cargo clean
