---
title: Release every verified main revision
blocked_by: []
blocks: []
tags: []
pr_mr_url: 
pr_mr_status: 
---

## Summary

Successful changes on main currently do not reliably produce a release. The configured version update is ignored, and publication begins independently of the full verification workflow. Make release delivery a dependable consequence of the exact main revision passing all checks, so users receive each naturally calculated semantic version without manual intervention.

## Context / Why

## Acceptance criteria

- [ ] The release workflow actually runs release-plz 0.3.159 update and applies the natural semantic version and changelog derived from conventional commit history.
- [ ] No release preparation or publication can begin until the exact triggering main revision passes the repository's full verification gate.
- [ ] A failed verification run creates no release commit, tag, draft GitHub release, crates.io publication, or public GitHub release.

## Subtasks

## Notes / Log
