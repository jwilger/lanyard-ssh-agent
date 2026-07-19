set dotenv-load := false

default:
    @just --list

fmt:
    cargo fmt --all --check
    nix fmt -- --ci

lint:
    cargo clippy --all-targets --all-features -- -D warnings
    actionlint

test:
    cargo test --all-features

e2e:
    cargo test --all-features --test e2e_openssh -- --ignored

deny:
    cargo deny check

mutants:
    cargo mutants --all-features --in-place

check: fmt lint test e2e deny mutants
