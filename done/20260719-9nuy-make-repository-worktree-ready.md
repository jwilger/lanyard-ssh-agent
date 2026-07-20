---
title: Make repository worktree-ready
blocked_by: [20260720-e3y3-make-release-test-shims-portable-in-nix-sandboxes]
blocks: []
tags: [tooling, worktrees]
pr_mr_url: 
pr_mr_status: 
---

## Summary

Make Lanyard consistently use isolated Git worktrees for ticket implementation, with tested lifecycle automation and guards that prevent commits or pushes from the primary checkout.

## Context / Why

Use the worktrees plugin's project-specific setup. Integrate the confirmed command surface through the existing justfile. Rust/Nix and Astro/npm caches should be warm without sharing mutable build state unsafely; this repository has no long-running app services or fixed development ports to isolate today.

## Acceptance criteria

- [x] Bootstrap and teardown scripts create and remove isolated ticket worktrees under the documented checkout root
- [x] The existing justfile exposes confirmed worktree lifecycle commands
- [x] Pre-commit and pre-push guards reject operations from the primary checkout while permitting worktrees
- [x] Rust/Nix and Astro/npm caches are warmed safely for new worktrees without copying secrets
- [x] Bats tests cover bootstrap, teardown, and guard behavior
- [x] AGENTS.md documents the required ticket-worktree workflow

## Subtasks

## Notes / Log

- 2026-07-20: Delivered directly to origin/main in signed commit 974ced6bb92efcd0039bed5550b3dbbb494fce69 (no remote topic branch or PR). Added isolated lifecycle commands and guards, safe cache warming, kernel-managed crash-safe port locking, consistent categorical trunk-publication docs, and 24 Bats tests. Local full gate, Nix flake, Playwright 8/8, formal final review, and exact pushed GitHub CI/Release/Pages/Tiber workflows all passed.
