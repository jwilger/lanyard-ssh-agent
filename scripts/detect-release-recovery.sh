#!/usr/bin/env bash
set -euo pipefail

signing_key_path="$RUNNER_TEMP/recovery-signing-key"
allowed_signers_path="$RUNNER_TEMP/recovery-allowed-signers"
release_state_path="$RUNNER_TEMP/recovery-github-release.json"

cleanup() {
  rm -f "$signing_key_path" "$allowed_signers_path" "$release_state_path"
}
trap cleanup EXIT

tag="$(git tag --list 'v[0-9]*' --sort=-version:refname | head -n 1)"
if [[ -z "$tag" ]]; then
  echo "recovering=false" >> "$GITHUB_OUTPUT"
  exit 0
fi

printf '%s\n' "$RELEASE_SIGNING_KEY" > "$signing_key_path"
chmod 600 "$signing_key_path"
printf '%s %s\n' \
  "${RELEASE_SIGNING_EMAIL:-release-plz-bot@users.noreply.github.com}" \
  "$(ssh-keygen -y -f "$signing_key_path")" > "$allowed_signers_path"
git config gpg.format ssh
git config gpg.ssh.allowedSignersFile "$allowed_signers_path"
git verify-tag "$tag"
release_commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
version="${tag#v}"

crate_status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  --retry 3 --retry-delay 2 --retry-all-errors --connect-timeout 10 --max-time 45 \
  --header 'User-Agent: lanyard-ssh-agent-release-check (https://github.com/jwilger/lanyard-ssh-agent)' \
  "https://crates.io/api/v1/crates/lanyard-ssh-agent/${version}")"
case "$crate_status" in
  200) crate_published=true ;;
  404) crate_published=false ;;
  *)
    echo "crates.io version check returned HTTP ${crate_status}" >&2
    exit 1
    ;;
esac

github_release_status="$(curl --silent --show-error --output "$release_state_path" \
  --write-out '%{http_code}' --retry 3 --retry-delay 2 --retry-all-errors \
  --connect-timeout 10 --max-time 45 \
  --header "Authorization: Bearer ${GH_RELEASE_AUTOMATION_TOKEN}" \
  --header 'Accept: application/vnd.github+json' \
  --header 'X-GitHub-Api-Version: 2022-11-28' \
  --header 'User-Agent: lanyard-ssh-agent-release-check' \
  "https://api.github.com/repos/${GITHUB_REPOSITORY:?}/releases/tags/${tag}")"
case "$github_release_status" in
  200)
    release_is_draft="$(
      jq --raw-output 'if (.draft | type) == "boolean" then .draft else empty end' \
        "$release_state_path"
    )"
    case "$release_is_draft" in
      true) ;;
      false)
        if [[ "$crate_published" != true ]]; then
          echo "GitHub Release ${tag} is public but crates.io version ${version} is missing" >&2
          exit 1
        fi
        echo "recovering=false" >> "$GITHUB_OUTPUT"
        exit 0
        ;;
      *)
        echo "GitHub Release ${tag} returned an invalid draft state" >&2
        exit 1
        ;;
    esac
    ;;
  404) ;;
  *)
    echo "GitHub Release check returned HTTP ${github_release_status}" >&2
    exit 1
    ;;
esac

{
  echo "recovering=true"
  echo "tag=${tag}"
  echo "release-commit=${release_commit}"
} >> "$GITHUB_OUTPUT"
