run:
    cargo build && cargo run

build:
    cargo clean && cargo build

test:
    cargo test -q
