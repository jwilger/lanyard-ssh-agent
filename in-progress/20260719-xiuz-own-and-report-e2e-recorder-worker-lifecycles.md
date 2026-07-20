---
title: Own and report E2E recorder worker lifecycles
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

Make the real-agent recording proxy own, join, and surface failures from every per-connection worker.

## Context / Why

Final-review finding architecture-maintainability-001 from 20260719-bebi.

## Acceptance criteria

- [x] Recorder teardown waits for all connection workers without deadlock
- [ ] Forwarding worker errors are surfaced in E2E diagnostics

## Subtasks

## Notes / Log
