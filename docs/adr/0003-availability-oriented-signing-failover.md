# 3. Prefer bounded availability for signing

Date: 2026-07-19

Status: accepted

## Context

Forwarded and local desktop agents may be locked, stale, or unavailable. Some
agents report user denial and generic failure identically.

## Decision

For a requested key, try candidate agents in policy order. Lanyard does not
cache which identities each backend previously advertised: availability and
ownership can change between requests, so every current candidate remains
eligible. Advance after a bounded timeout or generic failure, promote the first
successful signer for that exact public-key blob, and fail closed after every
candidate fails. Keep at most 32 key-specific signer affinities. Use a 500 ms
connect timeout, two seconds for identity listing, 30 seconds per signing
attempt, a 256 KiB maximum protocol message, and at most 32 candidates.

## Consequences

A denial at one backend may fall through to another backend that owns the same
key. This is an explicit availability tradeoff caused by protocol ambiguity.
Trying candidates without an identity cache can contact an agent that does not
own the requested key, but avoids stale ownership decisions and cannot produce
a signature unless an upstream agent actually signs.
