#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${ARTIFACT_DIR:-artifacts}"
release_state_path="$(mktemp)"
selected_release_path="${release_state_path}.selected"
release_page_path="${release_state_path}.page"
trap 'rm -f "$release_state_path" "$selected_release_path" "$release_page_path"' EXIT

printf '%s\n' '[]' > "$release_state_path"
page=1
while :; do
  release_status="$(curl --silent --show-error --output "$release_page_path" \
    --write-out '%{http_code}' --retry 3 --retry-delay 2 --retry-all-errors \
    --connect-timeout 10 --max-time 45 \
    --header "Authorization: Bearer ${GH_TOKEN:?}" \
    --header 'Accept: application/vnd.github+json' \
    --header 'X-GitHub-Api-Version: 2022-11-28' \
    --header 'User-Agent: lanyard-ssh-agent-release-staging' \
    "https://api.github.com/repos/${GITHUB_REPOSITORY:?}/releases?per_page=100&page=${page}")"
  if [[ "$release_status" != 200 ]]; then
    echo "GitHub Releases lookup returned HTTP ${release_status}" >&2
    exit 1
  fi
  release_count="$(jq 'if type == "array" then length else error("expected an array") end' "$release_page_path")"
  jq --slurp '.[0] + .[1]' "$release_state_path" "$release_page_path" > "$selected_release_path"
  mv "$selected_release_path" "$release_state_path"
  if ((release_count < 100)); then
    break
  fi
  ((page += 1))
done

matching_releases="$(
  jq --arg tag "$RELEASE_TAG" '[.[] | select(.tag_name == $tag)] | length' \
    "$release_state_path"
)"
case "$matching_releases" in
  0)
    release_flags=(--draft --verify-tag --generate-notes --title "$RELEASE_TAG" --target "$RELEASE_COMMIT")
    if [[ "$RELEASE_TAG" == *-* ]]; then
      release_flags+=(--prerelease)
    fi
    gh release create "$RELEASE_TAG" "${release_flags[@]}"
    upload_paths=("$artifact_dir"/*)
    ;;
  1)
    jq --arg tag "$RELEASE_TAG" '.[] | select(.tag_name == $tag)' \
      "$release_state_path" > "$selected_release_path"
    mv "$selected_release_path" "$release_state_path"
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
    artifact_paths=("$artifact_dir"/*)
    declare -A local_artifacts=()
    for artifact in "${artifact_paths[@]}"; do
      local_artifacts["$(basename "$artifact")"]="$artifact"
    done
    declare -A remote_digests=()
    while IFS=$'\t' read -r remote_name remote_digest; do
      if [[ -z "${local_artifacts[$remote_name]+present}" ]]; then
        echo "GitHub Release ${RELEASE_TAG} has unexpected asset ${remote_name}" >&2
        exit 1
      fi
      if [[ -z "$remote_digest" ]]; then
        echo "GitHub Release ${RELEASE_TAG} asset ${remote_name} has no digest" >&2
        exit 1
      fi
      remote_digests["$remote_name"]="$remote_digest"
    done < <(jq --raw-output '.assets[]? | [.name, (.digest // "")] | @tsv' "$release_state_path")

    upload_paths=()
    for artifact in "$artifact_dir"/*; do
      artifact_name="$(basename "$artifact")"
      if [[ -z "${remote_digests[$artifact_name]+present}" ]]; then
        upload_paths+=("$artifact")
        continue
      fi
      local_digest="sha256:$(sha256sum "$artifact" | cut -d ' ' -f 1)"
      if [[ "${remote_digests[$artifact_name]}" != "$local_digest" ]]; then
        echo "GitHub Release ${RELEASE_TAG} asset ${artifact_name} has digest ${remote_digests[$artifact_name]}, expected ${local_digest}" >&2
        exit 1
      fi
    done
    ;;
  *)
    echo "GitHub has ${matching_releases} releases tagged ${RELEASE_TAG}; refusing an ambiguous recovery" >&2
    exit 1
    ;;
esac
if ((${#upload_paths[@]})); then
  gh release upload "$RELEASE_TAG" "${upload_paths[@]}"
else
  echo "GitHub Release ${RELEASE_TAG} already has all staged artifacts"
fi
