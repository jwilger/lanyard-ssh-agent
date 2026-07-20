---
title: Install SIGTERM handling before publishing daemon sockets
blocked_by: []
blocks: []
tags: []
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Eliminate the startup race where agent.sock becomes connectable before the daemon has installed its SIGTERM handler, allowing an immediate service stop to exit by signal instead of cleaning up sockets.

## Context / Why

Discovered while building the Home Manager module check in an optimized Nix build: serve_recovers_a_stale_socket failed because SIGTERM arrived after bind but before shutdown_signal was polled.

## Acceptance criteria

- [ ] SIGTERM handling is installed before agent.sock or control.sock becomes externally connectable
- [ ] An immediate SIGTERM after socket readiness exits successfully and removes owned sockets
- [ ] The optimized Nix package test suite passes reliably

## Subtasks

## Notes / Log
