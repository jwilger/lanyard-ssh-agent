#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${ARTIFACT_DIR:-artifacts}"
release_state_path="$(mktemp)"
selected_release_path="${release_state_path}.selected"
release_page_path="${release_state_path}.page"
asset_names_path="${release_state_path}.assets"
trap 'rm -f "$release_state_path" "$selected_release_path" "$release_page_path" "$asset_names_path"' EXIT

shopt -s nullglob
if [[ ! -d "$artifact_dir" ]]; then
  echo "Artifact directory ${artifact_dir} does not exist" >&2
  exit 1
fi
artifact_paths=("$artifact_dir"/*)
if ((${#artifact_paths[@]} == 0)); then
  echo "Artifact directory ${artifact_dir} is empty" >&2
  exit 1
fi
declare -A expected_assets=()
for artifact in "${artifact_paths[@]}"; do
  if [[ ! -f "$artifact" || ! -r "$artifact" ]]; then
    echo "Artifact ${artifact} is not a readable regular file" >&2
    exit 1
  fi
  expected_assets["$(basename "$artifact")"]=1
done

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
create_draft() {
  release_flags=(--draft --verify-tag --generate-notes --title "$RELEASE_TAG" --target "$RELEASE_COMMIT")
  if [[ "$RELEASE_TAG" == *-* ]]; then
    release_flags+=(--prerelease)
  fi
  gh release create "$RELEASE_TAG" "${release_flags[@]}"
  upload_paths=("${artifact_paths[@]}")
}

case "$matching_releases" in
  0)
    create_draft
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
    existing_target="$(jq --raw-output 'if (.target_commitish | type) == "string" then .target_commitish else empty end' "$release_state_path")"
    if [[ "$existing_target" != "$RELEASE_COMMIT" ]]; then
      echo "GitHub Release ${RELEASE_TAG} targets ${existing_target:-<invalid>}, expected ${RELEASE_COMMIT}" >&2
      exit 1
    fi
    if ! jq --raw-output \
      'if (.assets | type) == "array" and all(.assets[]; (.name | type) == "string") then .assets[].name else error("invalid assets") end' \
      "$release_state_path" > "$asset_names_path"; then
      echo "GitHub Release ${RELEASE_TAG} returned invalid assets" >&2
      exit 1
    fi
    mapfile -t remote_names < "$asset_names_path"
    asset_sets_match=true
    if ((${#remote_names[@]} != ${#artifact_paths[@]})); then
      asset_sets_match=false
    fi
    for remote_name in "${remote_names[@]}"; do
      if [[ -z "${expected_assets[$remote_name]+present}" ]]; then
        asset_sets_match=false
      fi
    done
    if [[ "$asset_sets_match" == true ]]; then
      upload_paths=()
    else
      release_id="$(jq --raw-output 'if (.id | type) == "number" then .id else empty end' "$release_state_path")"
      if [[ ! "$release_id" =~ ^[0-9]+$ ]]; then
        echo "GitHub Release ${RELEASE_TAG} has an invalid release id" >&2
        exit 1
      fi
      gh api --method DELETE "repos/${GITHUB_REPOSITORY}/releases/${release_id}"
      create_draft
    fi
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
