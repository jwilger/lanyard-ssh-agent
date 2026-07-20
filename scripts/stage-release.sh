#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${ARTIFACT_DIR:-artifacts}"
release_state_path="$(mktemp)"
trap 'rm -f "$release_state_path"' EXIT

release_status="$(curl --silent --show-error --output "$release_state_path" \
  --write-out '%{http_code}' --retry 3 --retry-delay 2 --retry-all-errors \
  --connect-timeout 10 --max-time 45 \
  --header "Authorization: Bearer ${GH_TOKEN:?}" \
  --header 'Accept: application/vnd.github+json' \
  --header 'X-GitHub-Api-Version: 2022-11-28' \
  --header 'User-Agent: lanyard-ssh-agent-release-staging' \
  "https://api.github.com/repos/${GITHUB_REPOSITORY:?}/releases/tags/${RELEASE_TAG}")"

case "$release_status" in
  200)
    existing_draft="$(
      jq --raw-output 'if (.draft | type) == "boolean" then .draft else empty end' \
        "$release_state_path"
    )"
    if [[ "$existing_draft" != true ]]; then
      if [[ "$existing_draft" != false ]]; then
        echo "GitHub Release ${RELEASE_TAG} returned an invalid draft state" >&2
        exit 1
      fi
      echo "Existing GitHub Release ${RELEASE_TAG} is already public" >&2
      exit 1
    fi
    ;;
  404)
    release_flags=(--draft --verify-tag --generate-notes --title "$RELEASE_TAG" --target "$RELEASE_COMMIT")
    if [[ "$RELEASE_TAG" == *-* ]]; then
      release_flags+=(--prerelease)
    fi
    gh release create "$RELEASE_TAG" "${release_flags[@]}"
    ;;
  *)
    echo "GitHub Release lookup returned HTTP ${release_status}" >&2
    exit 1
    ;;
esac
gh release upload "$RELEASE_TAG" "$artifact_dir"/* --clobber
