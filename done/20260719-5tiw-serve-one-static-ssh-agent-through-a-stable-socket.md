---
title: Serve one static SSH agent through a stable socket
blocked_by: [20260719-3jah-bootstrap-public-repository-and-engineering-harness]
blocks: [20260719-4rpk-aggregate-discovered-and-registered-agent-identities]
tags: []
pr_mr_url: 
pr_mr_status: 
---

## Summary

Implement the smallest protocol-aware proxy from one configured upstream agent to Lanyard's stable Unix socket.

## Context / Why

## Acceptance criteria

- [x] The daemon serves the documented stable socket
- [x] Identity listing and signing work through one static upstream
- [x] Unsupported mutation and lock operations fail closed
- [x] Protocol size and operation time limits are enforced

## Subtasks

## Notes / Log
