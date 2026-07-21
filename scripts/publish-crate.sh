#!/usr/bin/env bash
set -euo pipefail

: "${RELEASE_TAG:?RELEASE_TAG is required}"
: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is required}"

crate_name="lanyard-ssh-agent"
crate_version="${RELEASE_TAG#v}"
manifest_version="$(
  cargo metadata --no-deps --format-version 1 |
    jq --raw-output --arg name "$crate_name" '.packages[] | select(.name == $name) | .version'
)"
if [[ -z "$manifest_version" || "$manifest_version" != "$crate_version" ]]; then
  echo "release tag ${RELEASE_TAG} does not match ${crate_name} ${manifest_version:-<missing>}" >&2
  exit 1
fi
registry_url="https://crates.io/api/v1/crates/${crate_name}/${crate_version}"
poll_attempts="${PUBLISH_POLL_ATTEMPTS:-12}"
poll_delay_seconds="${PUBLISH_POLL_DELAY_SECONDS:-10}"

registry_status() {
  curl \
    --retry 3 \
    --retry-all-errors \
    --connect-timeout 10 \
    --max-time 30 \
    --silent \
    --show-error \
    --header 'User-Agent: lanyard-ssh-agent-release-check (https://github.com/jwilger/lanyard-ssh-agent)' \
    --output /dev/null \
    --write-out '%{http_code}' \
    "$registry_url"
}

status="$(registry_status)"
case "$status" in
  200)
    echo "${crate_name} ${crate_version} is already published"
    exit 0
    ;;
  404) ;;
  *)
    echo "crates.io returned unexpected HTTP status ${status}" >&2
    exit 1
    ;;
esac

publish_status=0
cargo publish --locked || publish_status=$?

for ((attempt = 1; attempt <= poll_attempts; attempt++)); do
  status="$(registry_status)"
  case "$status" in
    200)
      echo "${crate_name} ${crate_version} is visible on crates.io"
      exit 0
      ;;
    404)
      if ((attempt < poll_attempts)); then
        sleep "$poll_delay_seconds"
      fi
      ;;
    *)
      echo "crates.io returned unexpected HTTP status ${status} while polling" >&2
      exit 1
      ;;
  esac
done

if ((publish_status != 0)); then
  echo "cargo publish failed and ${crate_name} ${crate_version} did not become visible" >&2
  exit "$publish_status"
fi
echo "${crate_name} ${crate_version} did not become visible on crates.io" >&2
exit 1
