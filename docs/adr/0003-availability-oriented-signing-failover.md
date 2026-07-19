# 3. Prefer bounded availability for signing

Date: 2026-07-19

Status: accepted

## Context

Forwarded and local desktop agents may be locked, stale, or unavailable. Some
agents report user denial and generic failure identically.

## Decision

For a requested key, try candidate agents in policy order. Advance after a
bounded timeout or generic failure, promote the first successful signer, and
fail closed after every candidate fails. Use a 500 ms connect timeout, two
seconds for identity listing, 30 seconds per signing attempt, a 256 KiB maximum
protocol message, and at most 32 candidates.

## Consequences

A denial at one backend may fall through to another backend that owns the same
key. This is an explicit availability tradeoff caused by protocol ambiguity.
