default:
    @just --list

build:
    cargo build

build-release:
    cargo build --release

run *ARGS:
    cargo run -- {{ARGS}}

run-release *ARGS:
    cargo run --release -- {{ARGS}}

fmt:
    cargo fmt

lint:
    cargo clippy --all-targets --all-features -- -D warnings

check:
    cargo check --all-targets --all-features

nix-build:
    nix build

nix-shell:
    nix develop

nix-check:
    nix flake check

clean:
    cargo clean
    rm -rf target/
