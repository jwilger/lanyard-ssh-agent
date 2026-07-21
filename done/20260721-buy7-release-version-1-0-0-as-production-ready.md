---
title: Release version 1.0.0 as production-ready
blocked_by: []
blocks: []
tags: []
pr_mr_url: 
pr_mr_status: 
---

## Summary

Promote Lanyard to production-ready 1.0.0, update all public documentation and website status language, verify the release candidate, and deliver it through the repository release workflow.

## Context / Why

## Acceptance criteria

- [ ] Cargo and generated release metadata identify version 1.0.0.
- [ ] User-facing documentation and the website describe Lanyard as production-ready with no project-level pre-release warning.
- [ ] Repository checks pass and the 1.0.0 release is published through the configured direct-to-main workflow.

## Subtasks

## Notes / Log

- 2026-07-21: CI recovery complete: unchanged-SHA rerun attempt 2 of run 29876110037 succeeded. The v1.0.0 GitHub Release is public and non-prerelease, crates.io reports 1.0.0, signed tag v1.0.0 targets 7a0eaff71446cc95c51018e962f3fd88c48f71e0, and the production site displays PRODUCTION READY / Version 1.0.
