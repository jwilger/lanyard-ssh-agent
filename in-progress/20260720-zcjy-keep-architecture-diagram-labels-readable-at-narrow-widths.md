---
title: Keep architecture diagram labels readable at narrow widths
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

Prevent the output labels in the architecture-page routing diagram from overlapping when the illustration is displayed at narrower widths.

## Context / Why

The diagram currently compresses the labels SSH, Git signing, multiplexer, and session until adjacent words run together. Readers must be able to distinguish each output at every supported responsive width. Add a browser regression test at the affected viewport and adjust the responsive diagram layout while preserving the existing visual design.

## Acceptance criteria

- [x] The SSH, Git signing, multiplexer, and session labels remain visually distinct at the reproduced narrow viewport
- [x] The diagram remains readable without horizontal page overflow at supported desktop and mobile widths
- [ ] Existing site unit, build, accessibility, and browser tests remain green

## Subtasks

## Notes / Log
