#!/usr/bin/env bash
set -euo pipefail

: "${RELEASE_TAG:?RELEASE_TAG is required}"

if ! existing_draft="$(gh release view "$RELEASE_TAG" --json isDraft --jq .isDraft)"; then
  echo "Expected staged GitHub Release ${RELEASE_TAG} was not found" >&2
  exit 1
fi

case "$existing_draft" in
  true) gh release edit "$RELEASE_TAG" --draft=false ;;
  false) echo "GitHub Release ${RELEASE_TAG} is already public" ;;
  *)
    echo "GitHub Release ${RELEASE_TAG} returned an invalid draft state" >&2
    exit 1
    ;;
esac
