default:
    @just --list

fmt:
    cargo fmt --all

test:
    if cargo nextest --version >/dev/null 2>&1; then cargo nextest run; else cargo test; fi

run:
    cargo run --release
