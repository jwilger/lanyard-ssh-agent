#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${ARTIFACT_DIR:-artifacts}"
if existing_draft="$(gh release view "$RELEASE_TAG" --json isDraft --jq .isDraft 2>/dev/null)"; then
  if [[ "$existing_draft" != true ]]; then
    echo "Existing GitHub Release ${RELEASE_TAG} is already public" >&2
    exit 1
  fi
else
  release_flags=(--draft --verify-tag --generate-notes --title "$RELEASE_TAG" --target "$RELEASE_COMMIT")
  if [[ "$RELEASE_TAG" == *-* ]]; then
    release_flags+=(--prerelease)
  fi
  gh release create "$RELEASE_TAG" "${release_flags[@]}"
fi
gh release upload "$RELEASE_TAG" "$artifact_dir"/* --clobber
