---
title: Adopt Lanyard on gregor
blocked_by: [20260719-6t5b-automate-releases-artifacts-crates-io-and-github-pages, 20260719-a5jv-package-the-daemon-with-home-manager-and-systemd]
blocks: []
tags: []
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Integrate the released Home Manager module into gregor's separate NixOS configuration.

## Context / Why

## Acceptance criteria

- [x] The existing unrelated NixOS worktree change is preserved
- [ ] Local and forwarded sessions both use Lanyard's stable socket
- [ ] 1Password remains the local fallback and remote sessions register forwarding
- [ ] A real Zellij attach workflow validates the original use case

## Subtasks

## Notes / Log

- 2026-07-20: Configuration merged in nixos-config PR #9 (merge 0a0fcdf7): Lanyard v0.1.0 pinned; gregor Home Manager module enabled; stable SSH_AUTH_SOCK enforced; 1Password retained as service fallback. Independent review clean. nixfmt hook, git diff --check, full nix flake check, and nixos-rebuild dry-run --flake .#gregor all pass. Runtime activation and real local/forwarded Zellij validation remain.
