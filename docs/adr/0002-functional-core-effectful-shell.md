---
number: 2
title: Separate routing decisions from effects
status: Accepted
date: 2026-07-19
---

## Context

Agent discovery and Unix I/O are fallible, while ordering, deduplication, and
promotion should be deterministic and exhaustively tested.

## Decision

Keep protocol parsing and routing policy in a functional core. Confine socket
I/O, clocks, filesystem discovery, and process lifecycle to a thin effectful
shell. Model paths, keys, and backend identities with semantic types.

## Consequences

Most behavior is testable without credentials or timing-sensitive sockets.
