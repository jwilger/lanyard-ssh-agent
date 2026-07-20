#!/usr/bin/env bash
set -euo pipefail

publishing=false
tag=""
signing_key_path="$RUNNER_TEMP/release-signing-key"
allowed_signers_path="$RUNNER_TEMP/release-allowed-signers"

finish() {
  rm -f "$signing_key_path" "$allowed_signers_path"
  {
    echo "publishing=${publishing}"
    if [[ -n "$tag" ]]; then
      echo "tag=${tag}"
      echo "tag-flag=--tag=${tag}"
    fi
  } >> "$GITHUB_OUTPUT"
}
trap finish EXIT

git add Cargo.toml Cargo.lock CHANGELOG.md
version="$(cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name == "lanyard-ssh-agent") | .version')"
auth="$(printf 'x-access-token:%s' "$GH_RELEASE_AUTOMATION_TOKEN" | base64 -w0)"

configure_signing() {
  [[ -n "$RELEASE_SIGNING_KEY" ]]
  printf '%s\n' "$RELEASE_SIGNING_KEY" > "$signing_key_path"
  chmod 600 "$signing_key_path"
  printf '%s %s\n' \
    "${RELEASE_SIGNING_EMAIL:-release-plz-bot@users.noreply.github.com}" \
    "$(ssh-keygen -y -f "$signing_key_path")" > "$allowed_signers_path"
  git config gpg.format ssh
  git config gpg.ssh.allowedSignersFile "$allowed_signers_path"
  git config user.signingkey "$signing_key_path"
  git config commit.gpgsign true
  git config tag.gpgsign true
  git config user.name "${RELEASE_SIGNING_NAME:-release-plz-bot}"
  git config user.email "${RELEASE_SIGNING_EMAIL:-release-plz-bot@users.noreply.github.com}"
}

if ! git diff --cached --quiet; then
  configure_signing
  git commit -S -m "chore(release): prepare v${version}"
  git -c "http.https://github.com/.extraheader=AUTHORIZATION: basic ${auth}" push origin HEAD:main
  exit 0
fi

crate_status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  --retry 3 --retry-delay 2 --retry-all-errors --connect-timeout 10 --max-time 45 \
  --header 'User-Agent: lanyard-ssh-agent-release-check (https://github.com/jwilger/lanyard-ssh-agent)' \
  "https://crates.io/api/v1/crates/lanyard-ssh-agent/${version}")"
case "$crate_status" in
  200)
    echo "lanyard-ssh-agent ${version} is already published"
    exit 0
    ;;
  404) ;;
  *)
    echo "crates.io version check returned HTTP ${crate_status}" >&2
    exit 1
    ;;
esac

configure_signing
tag="v${version}"
if git rev-parse --verify --quiet "refs/tags/${tag}^{commit}" > /dev/null; then
  [[ "$(git rev-parse "refs/tags/${tag}^{commit}")" == "$(git rev-parse HEAD)" ]]
  git verify-tag "$tag"
else
  git tag -s -a "$tag" -m "Release ${tag}"
fi
git -c "http.https://github.com/.extraheader=AUTHORIZATION: basic ${auth}" \
  push origin refs/tags/"${tag}"
publishing=true
