# ADR 0004: Separate crate publishing from artifact hosting

## Status

Accepted

## Context

Lanyard must publish a Rust crate, create signed release tags, provide native
Linux binaries for two architectures, attach checksums, and keep the process
automated. release-plz understands Rust versions, changelogs, semver checks,
and crates.io publishing. dist understands cross-target artifact builds and
GitHub Release hosting. Both tools can create GitHub Releases, which would make
them race on the same tag if both capabilities were enabled.

The repository also deploys a separate Astro documentation site to GitHub
Pages. That deployment follows changes to `main`, not release tags.

## Decision

release-plz owns release preparation, crates.io publishing, and the signed
`vX.Y.Z` tag. Its GitHub Release creation is disabled. dist 0.32.0 reacts to
that tag, builds x86_64 and aarch64 GNU/Linux archives, generates SHA-256
checksums, and exclusively owns the GitHub Release.

The release-plz workflow is consumed from an immutable, signed revision of
`jwilger/gha-workflows`. Actions referenced directly by this repository are
pinned to full commit SHAs. Pages uses a dedicated least-privilege workflow
and the `github-pages` deployment environment.

The release-plz caller is pinned to shared-workflow revision
`017ca09a54090c441a12a0234e7bd0f96d485b8f`. That revision pins every nested
action to a full commit SHA and enforces the invariant in its own CI, so the
caller runs on every push to `main` without a separate feature gate. Pinning
only the outer workflow would not make mutable nested action tags safe in jobs
that handle publishing credentials.

## Consequences

- A release is initiated only by merging release-plz's prepared change.
- crates.io publishing and native artifact production are independent jobs
  joined by the signed tag.
- A failed dist run can be rerun without republishing the crate.
- Updating dist regenerates its workflow; generated action tags must be
  replaced with immutable SHAs before the change is accepted.
- dist itself is installed from versioned archives whose SHA-256 digests are
  pinned in the repository; repository write permission is isolated to the
  hosting job.
- Pages can deploy documentation between crate releases.

## Alternatives considered

Letting release-plz create the GitHub Release was rejected because dist's
generated host job also creates it. A hand-written matrix was rejected because
it would duplicate dist's target planning, packaging, and checksum behavior.
Publishing binaries from the Pages workflow was rejected because site changes
and version tags have different lifecycles and permissions.

## Revisit when

Revisit if release-plz and dist gain an explicitly coordinated release mode,
if the project becomes a multi-package workspace, or if GitHub Releases stops
being the binary distribution channel.
