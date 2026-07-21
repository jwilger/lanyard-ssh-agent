---
title: Verify staged release files before publication
blocked_by: []
blocks: []
tags: []
pr_mr_url: 
pr_mr_status: 
---

## Summary

## Context / Why

## Acceptance criteria

- [ ] Recovery does not publish a staged release when any expected remote file is demonstrably incomplete or differs from the authoritative staged artifact.
- [ ] A genuinely complete staged release remains recoverable without replacing it solely because a fresh build is not byte-for-byte reproducible.
- [ ] Automated tests cover matching filenames with invalid contents or incomplete upload state.

## Subtasks

## Notes / Log
