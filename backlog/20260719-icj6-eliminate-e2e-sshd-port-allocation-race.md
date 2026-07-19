---
title: Eliminate E2E sshd port allocation race
blocked_by: []
blocks: []
tags: []
pr_mr_url: 
pr_mr_status: 
---

## Summary

Make the real OpenSSH E2E reserve or retry its loopback sshd port so cooperative local concurrency cannot cause a flaky required gate.

## Context / Why

Final-review finding production-risk-footguns-002 from 20260719-bebi.

## Acceptance criteria

- [ ] The E2E does not rely on an unreserved bind-to-zero port remaining free until sshd starts
- [ ] A regression test or deterministic harness assertion covers port-claim retry/reservation behavior

## Subtasks

## Notes / Log
