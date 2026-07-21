---
title: Ensure released packages report the correct version
blocked_by: []
blocks: []
tags: [github-issue, release, nix]
pr_mr_url: 
pr_mr_status: 
---

## Summary

Published packages should identify themselves with the same version as the release users selected.

## Context / Why

## Acceptance criteria

- [ ] The Nix package metadata for a tagged release reports the same version as that tag and the packaged executable.
- [ ] An automated check fails when release package metadata retains a previous version.

## Subtasks

## Notes / Log

- 2026-07-21: Source: GitHub issue #3, https://github.com/jwilger/lanyard-ssh-agent/issues/3. Release v0.1.1 builds and runs version 0.1.1, but its Nix derivation metadata reports 0.1.0. This exposes stale version information to downstream package consumers. Desired outcome: every release reports consistent version information in both the executable and package metadata. Reproduction used tag v0.1.1 at commit 3051192f7c7190bdc6127953bf28861c7295c328; the flake listing reports lanyard-ssh-agent-0.1.0 while the binary reports 0.1.1. Investigate the release and version update path so future tags cannot retain stale derivation metadata.
