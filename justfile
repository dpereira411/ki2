set positional-arguments

default:
    just --list

run:
    cargo run

build:
    cargo build

check:
    cargo check

test:
    cargo test

fmt:
    cargo fmt

clippy:
    cargo clippy --all-targets --all-features -- -D warnings
