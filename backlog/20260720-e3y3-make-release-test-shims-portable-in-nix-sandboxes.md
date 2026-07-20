---
title: Make release test shims portable in Nix sandboxes
blocked_by: []
blocks: []
tags: [testing, nix, release]
pr_mr_url: 
pr_mr_status: 
---

## Summary

Make dynamically generated release-test executables run inside pure Nix build sandboxes.

## Context / Why

nix flake check rebuilds the package after any source change. Eleven release_automation tests fail because their temporary shims use #!/usr/bin/env bash while /usr/bin/env is absent in the pure build sandbox; the same tests pass in nix develop. Fix test infrastructure only, without changing release behavior.

## Acceptance criteria

- [ ] Runtime-generated release test shims use a shell path available in pure Nix builds
- [ ] The full release_automation test target passes inside the package derivation

## Subtasks

## Notes / Log
