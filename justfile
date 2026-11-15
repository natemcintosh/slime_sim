default:
    @just --list

alias f := fmt
fmt:
    cargo fmt --all

alias t := test
test:
    if cargo nextest --version >/dev/null 2>&1; then cargo nextest run; else cargo test; fi

alias r := run
run:
    cargo run --release
