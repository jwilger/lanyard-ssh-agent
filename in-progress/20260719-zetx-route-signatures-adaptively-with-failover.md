---
title: Route signatures adaptively with failover
blocked_by: [20260719-4rpk-aggregate-discovered-and-registered-agent-identities]
blocks: [20260719-bebi-verify-end-to-end-ssh-authentication-and-git-signing]
tags: []
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Select the first viable owner of a signing key, retry safely, and promote successful signers.

## Context / Why

## Acceptance criteria

- [x] Forwarded or discovered candidates are initially preferred over static fallback
- [x] Successful signing promotes that backend without identity-list side effects
- [x] Timeouts and generic failures advance to the next candidate
- [ ] Lanyard fails closed when no candidate signs

## Subtasks

## Notes / Log
