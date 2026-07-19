# 2. Separate routing decisions from effects

Date: 2026-07-19

Status: accepted

## Context

Agent discovery and Unix I/O are fallible, while ordering, deduplication, and
promotion should be deterministic and exhaustively tested.

## Decision

Keep protocol parsing and routing policy in a functional core. Confine socket
I/O, clocks, filesystem discovery, and process lifecycle to a thin effectful
shell. Model paths, keys, and backend identities with semantic types.

## Consequences

Most behavior is testable without credentials or timing-sensitive sockets.
