---
title: Make release test shims portable in Nix sandboxes
blocked_by: []
blocks: [20260719-9nuy-make-repository-worktree-ready]
tags: [testing, nix, release]
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Make dynamically generated release-test executables run inside pure Nix build sandboxes.

## Context / Why

nix flake check rebuilds the package after any source change. Eleven release_automation tests fail because their temporary shims use #!/usr/bin/env bash while /usr/bin/env is absent in the pure build sandbox; the same tests pass in nix develop. Fix test infrastructure only, without changing release behavior.

## Acceptance criteria

- [x] Runtime-generated release test shims use a shell path available in pure Nix builds
- [x] The full release_automation test target passes inside the package derivation
- [x] nix flake check passes from a fresh package rebuild

## Subtasks

## Notes / Log

- 2026-07-20: Fixed on trunk at fa172f1. Generated release-test shims now resolve Bash from the trusted test PATH and execute under an empty environment; jq is declared as nativeCheckInputs for real release script parsing. cargo test --test release_automation passed 24/24, strict Clippy passed, a fresh pure Nix package derivation passed, nix flake check passed, independent final review was clean, and push-triggered CI 29753256040, Release 29753255810, and Tiber 29753256327 all completed successfully.
