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
- [ ] A successful eligible revision preserves the signed release commit and tag, both supported Linux archives and checksums, draft staging, crates.io publication, and public GitHub release as the final irreversible step.
- [ ] A verification run caused by a release-preparation commit is idempotent and cannot publish a duplicate release.
- [ ] Automated tests cover release triggering, verification gating, natural version calculation, and removal of the unsupported action-wrapper command.
- [ ] Operator and public documentation describe the verified-main-to-release flow and its recovery behavior.
- [ ] The next natural semantic version is observed end to end on crates.io and as a public GitHub release with matching signed provenance.

## Subtasks

## Notes / Log
