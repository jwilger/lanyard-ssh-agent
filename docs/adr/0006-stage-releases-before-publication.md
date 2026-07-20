# ADR 0006: Stage releases before publication

## Status

Accepted

## Context

Lanyard publishes one Rust crate and a matching set of cargo-dist artifacts.
crates.io publication is irreversible, while artifact builds and a draft
GitHub Release are safe to retry. The former split workflow could publish the
crate independently of artifact hosting and required a release pull request,
leaving ordering and recovery implicit.

The repository uses trunk-based development. A push to `main` should be enough
to prepare and deliver a release without a second workflow or coordination PR.

## Decision

One `Release` workflow owns the complete state machine:

1. release-plz prepares version and changelog changes only. A signed commit is
   pushed directly to `main` when preparation is required.
2. The publishing run creates or verifies a signed tag and resolves its commit
   as the authoritative release provenance.
3. Every cargo-dist job checks out that exact commit, builds checksummed Linux
   artifacts, and stages them in a draft GitHub Release.
4. The crates.io credential is loaded only after staging succeeds. The workflow
   verifies tag/manifest identity, publishes the crate, and waits for registry
   visibility.
5. The workflow makes the GitHub Release public only after crates.io succeeds.

Retries inspect external state. A published crate with a draft or missing
GitHub Release resumes only when an existing signed tag supplies authoritative
provenance. A completed release is a no-op. Missing or invalid provenance and
ambiguous API states fail closed.

GitHub Pages remains a separate deployment workflow because documentation
changes and software releases have different lifecycles and permissions.

## Consequences

- The draft GitHub Release exists before crates.io becomes irreversible.
- The workflow makes the GitHub Release public last.
- No release pull request or second release workflow is required.
- Interrupted runs can reuse a draft, skip an existing crate, and finish the
  announcement without rebuilding from a later `main` commit.
- release-plz no longer owns tagging, crate publication, or GitHub Releases; it
  only prepares version and changelog files.
- cargo-dist still supplies target planning and artifact construction, while
  small repository scripts make external state transitions explicit and
  testable.
- Credentials and repository write permissions are isolated to the steps and
  jobs that cross their corresponding trust boundary.

## Alternatives considered

A release-plz pull request was rejected because it adds a coordination branch
to an otherwise trunk-based delivery flow. Publishing crates.io first was
rejected because later artifact failures would leave an incomplete public
release. Making cargo-dist host a public release before crate publication was
rejected for the same reason. Two independent workflows were rejected because
their ordering and recovery state cannot be expressed as one job graph.

## Supersedes

ADR 0004.

## Revisit when

Revisit if crates.io supports transactional or reversible publication, or if a
single upstream tool can enforce the same staged ordering and signed-provenance
recovery semantics without custom orchestration.
