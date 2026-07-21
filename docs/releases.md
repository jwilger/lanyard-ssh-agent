# Releases and deployments

Lanyard uses one trunk-based CI pipeline. Every push to `main` first runs the
full repository check. Only when the full repository check succeeds does CI
call the `Release` state machine for that exact revision. No release pull request
is created or merged.

Rapid pushes can coalesce into one natural semantic-version release. Each run
uses only its verified revision: if a newer commit has already advanced
`main`, the older run's release-preparation push fails safely instead of
releasing unverified code. The newer successful run then includes the combined
unreleased commits in its naturally calculated version and changelog.

## Release sequence

1. CI completes `just check`, including formatting, linting, tests, end-to-end
   checks, dependency policy, and mutation testing.
2. release-plz updates `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`. When that
   produces a change, Lanyard creates a signed release-preparation commit,
   pushes it directly to `main`, and continues in the same non-cancelled run.
   The run triggered by that push will later observe the completed release and
   become a no-op. These are the only version-bearing files release-plz must
   edit: the Nix package reads its version directly from `Cargo.toml`.
3. The active run creates or verifies a signed annotated `vX.Y.Z` tag and
   resolves its commit. An existing verified tag remains authoritative if
   `main` has advanced. Every later checkout uses that exact commit.
4. cargo-dist builds the x86_64 and aarch64 GNU/Linux archives and SHA-256
   checksums. Those verified artifacts are uploaded to a draft GitHub
   Release. Nothing is public yet.
5. Only after the draft exists, the workflow loads the crates.io credential,
   validates that the tag and crate manifest versions match, and runs
   `cargo publish --locked`. It polls the public registry until that exact
   version is visible.
6. Only after crates.io succeeds does the workflow make the GitHub Release
   public.

The irreversible boundary is therefore late in the pipeline: artifacts are
built and safely staged before the crate is published, and the public release
is the final announcement.

## Retry and recovery behavior

The workflow serializes runs for `main` and does not cancel an in-progress
release. Its state transitions are idempotent:

- an existing draft is reused only when it has exactly the expected uniquely
  named assets, every upload is complete, and each downloaded file matches
  GitHub's recorded size and SHA-256 digest;
- recovery validates the bytes already staged on GitHub rather than comparing
  them with a fresh build, because release archives are not guaranteed to be
  byte-for-byte reproducible;
- a newly uploaded or replacement draft is reloaded from GitHub and each
  downloaded asset must match both GitHub's metadata and the local file that
  was just uploaded before publication can continue;
- an already-published crate is not published twice;
- a failed or ambiguous `cargo publish` is followed by a bounded crates.io
  visibility check;
- a draft or missing GitHub Release can resume only from its existing verified
  signed tag;
- an already-public release with a valid signed tag is a successful no-op;
- missing provenance, mismatched versions, unexpected API responses, invalid
  signatures, and an unexpectedly public release before crates.io all fail
  closed.

## Repository setup

Configure these repository resources:

- secret `OP_SERVICE_ACCOUNT_TOKEN`, with access to the `Github Secrets`
  1Password vault;
- variables `RELEASE_SIGNING_NAME` and `RELEASE_SIGNING_EMAIL`;
- `GH_RELEASE_AUTOMATION_TOKEN` and `RELEASE_SIGNING_KEY` items in that vault,
  used only while preparing signed commits and tags;
- a `CARGO_REGISTRY_TOKEN` item in that vault, loaded only by the job that runs
  after artifact staging;
- GitHub Pages with **GitHub Actions** selected as its source.

All third-party actions are pinned to immutable commit SHAs. Checkout
credentials are disabled, release credentials are scoped to the steps that
need them, and force pushes are never used. Branch and tag rules must continue
to require signed history and disallow force pushes.

## Pages

Pushes to `main` that change `site/` run the separate least-privilege Pages
workflow. It installs the locked npm dependency graph, builds the Astro site,
uploads `site/dist`, and deploys through the protected `github-pages`
environment. The workflow can also be dispatched manually.

## Local validation

Run the repository gate before changing release automation:

```console
just check
cargo package --allow-dirty
dist plan --output-format=json
```

`dist plan` must list both supported Linux archives and their `.sha256` files.
The generated `.github/workflows/release.yml` is checked in. Its action pins
come from `workspace.metadata.dist.github-action-commits`; update each entry to
the immutable commit SHA for the exact action release before regenerating.

The generated installer pipe is replaced by
`.github/actions/install-dist/action.yml`. That local action verifies dist's
versioned release archive against repository-pinned SHA-256 values before
extracting or executing it. Revalidate both architecture hashes when upgrading
dist.
