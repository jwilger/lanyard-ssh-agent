set dotenv-load := false

default:
    @just --list

fmt:
    cargo fmt --all --check
    nix fmt -- --ci

lint:
    cargo clippy --all-targets --all-features -- -D warnings
    actionlint
    shellcheck scripts/worktree-*.sh .githooks/post-checkout .githooks/pre-commit .githooks/pre-push tests/worktrees/test_helper.bash

test:
    cargo test --all-features
    bats tests/worktrees

e2e:
    cargo test --all-features --test e2e_openssh -- --ignored

deny:
    cargo deny check

mutants:
    cargo mutants --all-features --in-place

check: fmt lint test e2e deny mutants

worktree-create name start_point="HEAD":
    scripts/worktree-bootstrap.sh "{{name}}" "{{start_point}}"

worktree-remove name:
    scripts/worktree-teardown.sh "{{name}}"

worktree-list:
    git worktree list
