default:
    @just --list

check:
    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --no-deps -- -D warnings

build:
    cargo build --all-targets
    cargo test --all-targets

run *args:
    cargo run --bin virtual-matter-bridge {{args}}

commission *args:
    cargo run --bin dev-commission -- commission {{args}}

status:
    cargo run --bin dev-commission -- status

remove node_id:
    cargo run --bin dev-commission -- remove {{quote(node_id)}}

test:
    cargo test --all-targets

clean:
    cargo clean
