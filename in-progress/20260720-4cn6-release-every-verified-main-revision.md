---
title: Release every verified main revision
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

Successful changes on main currently do not reliably produce a release. The configured version update is ignored, and publication begins independently of the full verification workflow. Make release delivery a dependable consequence of the exact main revision passing all checks, so users receive each naturally calculated semantic version without manual intervention.

## Context / Why

## Acceptance criteria

- [x] The release workflow actually runs release-plz 0.3.159 update and applies the natural semantic version and changelog derived from conventional commit history.
- [x] No release preparation or publication can begin until the exact triggering main revision passes the repository's full verification gate.
- [ ] A failed verification run creates no release commit, tag, draft GitHub release, crates.io publication, or public GitHub release.
- [ ] A successful eligible revision preserves the signed release commit and tag, both supported Linux archives and checksums, draft staging, crates.io publication, and public GitHub release as the final irreversible step.
- [ ] A verification run caused by a release-preparation commit is idempotent and cannot publish a duplicate release.
- [ ] Automated tests cover release triggering, verification gating, natural version calculation, and removal of the unsupported action-wrapper command.
- [ ] Operator and public documentation describe the verified-main-to-release flow and its recovery behavior.
- [ ] The next natural semantic version is observed end to end on crates.io and as a public GitHub release with matching signed provenance.

## Subtasks

## Notes / Log

- 2026-07-20: Implementation notes: use release-plz 0.3.159 for natural version calculation. Keep the trunk-based state machine idempotent, preserve signed commits and tags, publish checksummed x86_64 and aarch64 Linux archives, stage GitHub as a draft, publish crates.io, and make GitHub public last.
- 2026-07-21: Release proof: CI run 29802526669 succeeded for 09113d16502864942da79a4dcefcd7653bf23e7a; crates.io 0.1.1 is visible; GitHub v0.1.1 is public with seven expected assets. Final failure record: fee9a9bf63dcd4737ef2a32583cceab6e7ab0b95, run 29802401356, check/Check failed because Clippy measured recovery test cognitive complexity 31/25. Diagnosis: caused by expanded cohesive state matrix. Next action was tested causal repair 09113d1 with rationale body. Replacement terminal status: success; queued/pending/running: none.
