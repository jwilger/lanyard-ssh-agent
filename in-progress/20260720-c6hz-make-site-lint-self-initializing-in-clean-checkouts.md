---
title: Make site lint self-initializing in clean checkouts
blocked_by: []
blocks: []
tags: [site, testing, tooling]
pr_mr_url: 
pr_mr_status: 
claim:
  host: unknown
  session: unknown
---

## Summary

Ensure the site lint command generates Astro type metadata before ESLint in a fresh checkout.

## Context / Why

After npm ci in a clean worktree, npm run check fails with unresolved import.meta.env types because lint runs before Astro creates ignored .astro metadata. Running the build first makes the unchanged lint pass. Fix the command lifecycle without weakening lint rules.

## Acceptance criteria

- [x] npm ci followed directly by npm run check passes in a clean checkout
- [x] Astro type generation runs before type-aware ESLint without weakening rules
- [x] Site unit, build, and browser tests remain green

## Subtasks

## Notes / Log
