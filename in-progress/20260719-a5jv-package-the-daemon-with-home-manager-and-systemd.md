---
title: Package the daemon with Home Manager and systemd
blocked_by: [20260719-bebi-verify-end-to-end-ssh-authentication-and-git-signing]
blocks: [20260719-7kjj-adopt-lanyard-on-gregor]
tags: []
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Ship a Nix package, Home Manager module, and systemd user service with safe shell and SSH integration.

## Context / Why

## Acceptance criteria

- [ ] The flake exposes packages, apps, checks, and a Home Manager module
- [ ] The user service owns the stable runtime and control sockets
- [ ] Shell integration registers incoming forwarded sockets before exporting Lanyard's socket
- [ ] Linux behavior is added without regressing Darwin configuration

## Subtasks

## Notes / Log
