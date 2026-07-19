---
title: Publish architecture decision records on the website
blocked_by: []
blocks: []
tags: [site, documentation, architecture]
pr_mr_url: 
pr_mr_status: 
---

## Summary

Publish the repository ADRs as navigable, styled pages in the Lanyard documentation site instead of mentioning records that visitors cannot read there.

## Context / Why

Use the canonical Markdown files under docs/adr as the source of truth. The site should expose an architecture index and a stable page for each ADR, preserve status/context/decision/consequences content, and integrate with the existing visual and accessibility system.

## Acceptance criteria

- [ ] Every canonical docs/adr Markdown record is published at a stable website URL
- [ ] An architecture index lists ADR number, title, and status and links to each record
- [ ] Existing website references to ADRs link to the published pages

## Subtasks

## Notes / Log
