# Releases and deployments

Lanyard uses one tag-driven release path. A release-plz pull request updates
the crate version and changelog. Merging that pull request causes release-plz
to publish `lanyard-ssh-agent` to crates.io and push a signed `vX.Y.Z` tag.
That tag starts dist, which creates the GitHub Release and attaches:

- `lanyard-ssh-agent-x86_64-unknown-linux-gnu.tar.xz` and its SHA-256 file;
- `lanyard-ssh-agent-aarch64-unknown-linux-gnu.tar.xz` and its SHA-256 file;
- the source archive, its SHA-256 file, and `sha256.sum`.

release-plz deliberately does not create the GitHub Release. Keeping that
responsibility in dist prevents two independent jobs from racing to create the
same release.

## Repository setup

The `Release` workflow calls the signed, immutable
`jwilger/gha-workflows` revision
`017ca09a54090c441a12a0234e7bd0f96d485b8f`, recorded in
`.github/workflows/release-plz.yml`. Configure these repository resources:

- secret `OP_SERVICE_ACCOUNT_TOKEN`, with access to the shared `Github Secrets`
  1Password vault;
- variables `RELEASE_SIGNING_NAME` and `RELEASE_SIGNING_EMAIL`, used by the
  shared workflow for signed commits and tags;
- a crates.io API token stored as `CARGO_REGISTRY_TOKEN` in that vault;
- GitHub Pages with **GitHub Actions** selected as its source.

The shared workflow also retrieves its GitHub automation token and SSH signing
key from 1Password. Branch and tag rules must continue to require signed
history and disallow force pushes.

### Release trigger

Every push to `main` invokes the reusable workflow. Its credential-bearing
nested actions are pinned to full immutable commit SHAs and a shared-workflow
CI policy rejects mutable references. The release-PR and publish jobs retrieve
only their required credentials from 1Password. A run with no release work is
a successful no-op.

## Pages

Pushes to `main` that change `site/` run the Pages workflow. It installs the
locked npm dependency graph, builds the Astro site, uploads `site/dist`, and
deploys through the protected `github-pages` environment. The workflow can
also be dispatched manually.

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
the immutable commit SHA for the exact action release before regenerating. The
CI file has small shellcheck hardening edits beyond dist's template, so review
the diff and restore those edits after running `dist generate`.

The generated installer pipe is also replaced by
`.github/actions/install-dist/action.yml`. That local action verifies dist's
versioned release archive against repository-pinned SHA-256 values before
extracting or executing it. Revalidate and update both architecture hashes when
upgrading dist. Only the `host` job receives `contents: write`; build jobs run
with read-only repository permission.
