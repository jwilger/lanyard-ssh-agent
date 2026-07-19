---
title: Aggregate discovered and registered agent identities
blocked_by: [20260719-5tiw-serve-one-static-ssh-agent-through-a-stable-socket]
blocks: [20260719-zetx-route-signatures-adaptively-with-failover]
tags: []
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Discover forwarded OpenSSH sockets and support explicit runtime registration while presenting a deduplicated identity union.

## Context / Why

## Acceptance criteria

- [x] Forwarded agents can be auto-discovered and explicitly registered or unregistered
- [ ] The static 1Password agent remains available as fallback
- [ ] Identity results are deduplicated and candidate count is bounded
- [ ] Status and socket commands expose stable machine-readable state

## Subtasks

## Notes / Log
