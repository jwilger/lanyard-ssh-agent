---
title: Make repository worktree-ready
blocked_by: [20260720-e3y3-make-release-test-shims-portable-in-nix-sandboxes]
blocks: []
tags: [tooling, worktrees]
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Make Lanyard consistently use isolated Git worktrees for ticket implementation, with tested lifecycle automation and guards that prevent commits or pushes from the primary checkout.

## Context / Why

Use the worktrees plugin's project-specific setup. Integrate the confirmed command surface through the existing justfile. Rust/Nix and Astro/npm caches should be warm without sharing mutable build state unsafely; this repository has no long-running app services or fixed development ports to isolate today.

## Acceptance criteria

- [x] Bootstrap and teardown scripts create and remove isolated ticket worktrees under the documented checkout root
- [x] The existing justfile exposes confirmed worktree lifecycle commands
- [x] Pre-commit and pre-push guards reject operations from the primary checkout while permitting worktrees
- [ ] Rust/Nix and Astro/npm caches are warmed safely for new worktrees without copying secrets
- [ ] Bats tests cover bootstrap, teardown, and guard behavior
- [ ] AGENTS.md documents the required ticket-worktree workflow

## Subtasks

## Notes / Log
