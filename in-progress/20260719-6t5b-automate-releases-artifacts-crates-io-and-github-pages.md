---
title: Automate releases, artifacts, crates.io, and GitHub Pages
blocked_by: []
blocks: [20260719-7kjj-adopt-lanyard-on-gregor]
tags: []
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Automate versioning, signed release artifacts, crates.io publication, and Pages deployment.

## Context / Why

## Acceptance criteria

- [ ] release-plz and the pinned reusable workflow drive releases from main
- [x] cargo-dist publishes Linux x86_64 and aarch64 artifacts with checksums
- [x] The crate package and GitHub Release are reproducible from the repository
- [x] GitHub Pages deploys the versioned documentation site

## Subtasks

## Notes / Log

- 2026-07-19: Pages deployment, crates.io packaging, and cargo-dist artifact automation are landed and verified in main. Remaining blocker: every available pinned jwilger/gha-workflows release-plz revision invokes mutable nested action tags in jobs handling release credentials. The caller remains intentionally fail-closed until that external workflow is fixed and repinned; acceptance criterion 1 is therefore incomplete.
