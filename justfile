run:
    cargo build -p thndrs && cargo run -p thndrs

build:
    cargo clean && cargo build -p thndrs

test:
    cargo test -q -p thndrs
