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

- [x] cargo-dist publishes Linux x86_64 and aarch64 artifacts with checksums
- [x] The crate package and GitHub Release are reproducible from the repository
- [x] GitHub Pages deploys the versioned documentation site
- [ ] A single main-branch workflow prepares signed release commits without a release PR, stages verified cargo-dist artifacts in a draft GitHub Release, publishes to crates.io, and only then makes the GitHub Release public

## Subtasks

## Notes / Log

- 2026-07-19: Pages deployment, crates.io packaging, and cargo-dist artifact automation are landed and verified in main. Remaining blocker: every available pinned jwilger/gha-workflows release-plz revision invokes mutable nested action tags in jobs handling release credentials. The caller remains intentionally fail-closed until that external workflow is fixed and repinned; acceptance criterion 1 is therefore incomplete.
- 2026-07-19: 2026-07-19: Enabled the signed, SHA-pinned shared release workflow and verified 1Password secret loading, signing setup, and release-plz CLI execution. crates.io publication of lanyard-ssh-agent 0.1.0 succeeded, but branch and tag creation returned HTTP 403 because GH_RELEASE_AUTOMATION_TOKEN lacks Contents/Pull requests write access to jwilger/lanyard-ssh-agent. No v0.1.0 tag or GitHub Release exists yet. Shared-workflow PR https://github.com/jwilger/gha-workflows/pull/9 is CI-green and still awaits the repository-required external approval.
- 2026-07-20: CI recovery — Failure record: bfeab441367b707efd24fdf880769deda5ccface; run 29723819817; exact failed job check; failed step Check; actionlint reported constant expression false in .github/workflows/release.yml. Diagnosis: the temporary inert legacy-plan guard violated the actionlint gate; classification=caused; focused regression and actionlint checks proved the event-based replacement. Next action: tested causal repair 71dbec21d9056e06df2b4d6ddb6928e1ee8b407a. Release proof: replacement run 29724239704; terminal status=success; queued|pending|running=false.
