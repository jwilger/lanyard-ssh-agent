#!/usr/bin/env bash
set -euo pipefail

publishing=false
tag=""
release_commit=""
signing_key_path="$RUNNER_TEMP/release-signing-key"
allowed_signers_path="$RUNNER_TEMP/release-allowed-signers"
release_state_path="$RUNNER_TEMP/github-release-state.json"

finish() {
  rm -f "$signing_key_path" "$allowed_signers_path" "$release_state_path"
  {
    echo "publishing=${publishing}"
    if [[ -n "$tag" ]]; then
      echo "tag=${tag}"
      echo "tag-flag=--tag=${tag}"
    fi
    if [[ -n "$release_commit" ]]; then
      echo "release-commit=${release_commit}"
    fi
  } >> "$GITHUB_OUTPUT"
}
trap finish EXIT

configure_verification() {
  [[ -n "$RELEASE_SIGNING_KEY" ]]
  printf '%s\n' "$RELEASE_SIGNING_KEY" > "$signing_key_path"
  chmod 600 "$signing_key_path"
  printf '%s %s\n' \
    "${RELEASE_SIGNING_EMAIL:-release-plz-bot@users.noreply.github.com}" \
    "$(ssh-keygen -y -f "$signing_key_path")" > "$allowed_signers_path"
  git config gpg.format ssh
  git config gpg.ssh.allowedSignersFile "$allowed_signers_path"
}

configure_signing() {
  configure_verification
  git config user.signingkey "$signing_key_path"
  git config commit.gpgsign true
  git config tag.gpgsign true
  git config user.name "${RELEASE_SIGNING_NAME:-release-plz-bot}"
  git config user.email "${RELEASE_SIGNING_EMAIL:-release-plz-bot@users.noreply.github.com}"
}

if [[ -n "${RECOVERY_TAG:-}" ]]; then
  tag="$RECOVERY_TAG"
  configure_verification
  if ! git rev-parse --verify --quiet "refs/tags/${tag}^{commit}" > /dev/null; then
    echo "Recovery tag ${tag} is missing" >&2
    exit 1
  fi
  git verify-tag "$tag"
  release_commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
  publishing=true
  exit 0
fi

git add Cargo.toml Cargo.lock CHANGELOG.md
version="$(cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name == "lanyard-ssh-agent") | .version')"
auth="$(printf 'x-access-token:%s' "$GH_RELEASE_AUTOMATION_TOKEN" | base64 -w0)"

if ! git diff --cached --quiet; then
  configure_signing
  git commit -S -m "chore(release): prepare v${version}"
  git -c "http.https://github.com/.extraheader=AUTHORIZATION: basic ${auth}" push origin HEAD:main
fi

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

tag="v${version}"
if [[ "$crate_published" == true ]]; then
  configure_verification
  if ! git rev-parse --verify --quiet "refs/tags/${tag}^{commit}" > /dev/null; then
    echo "Published crate ${version} has no authoritative ${tag} tag" >&2
    exit 1
  fi
  release_commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
  git verify-tag "$tag"

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
          tag=""
          echo "lanyard-ssh-agent ${version} and its GitHub Release are already published"
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

  publishing=true
  exit 0
fi

configure_signing
if git rev-parse --verify --quiet "refs/tags/${tag}^{commit}" > /dev/null; then
  release_commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
  git verify-tag "$tag"
else
  git tag -s -a "$tag" -m "Release ${tag}"
  release_commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
fi
git -c "http.https://github.com/.extraheader=AUTHORIZATION: basic ${auth}" \
  push origin refs/tags/"${tag}"
publishing=true
