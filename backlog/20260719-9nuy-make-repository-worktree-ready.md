---
title: Make repository worktree-ready
blocked_by: []
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

- [ ] Bootstrap and teardown scripts create and remove isolated ticket worktrees under the documented checkout root
- [ ] The existing justfile exposes confirmed worktree lifecycle commands

## Subtasks

## Notes / Log
