#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: worktree-ports.sh <worktree-path> | worktree-ports.sh release <worktree-path>" >&2
  exit 2
}

if [ "${1-}" = "release" ]; then
  [ "$#" -eq 2 ] || usage
  mode=release
  worktree="$2"
else
  [ "$#" -eq 1 ] || usage
  mode=allocate
  worktree="$1"
fi

script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
common_dir="$(git -C "$script_root" rev-parse --path-format=absolute --git-common-dir)"
if [ -d "$worktree" ]; then
  worktree="$(cd "$worktree" && pwd -P)"
  target_git_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-dir)"
  target_common_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-common-dir)"
  target_root="$(git -C "$worktree" rev-parse --path-format=absolute --show-toplevel)"
  if [ "$target_common_dir" != "$common_dir" ]; then
    echo "worktrees: target belongs to a different repository: $worktree" >&2
    exit 1
  fi
  if [ "$mode" = allocate ] \
    && { [ "$target_git_dir" = "$target_common_dir" ] || [ "$target_root" != "$worktree" ]; }; then
    echo "worktrees: target must be a linked worktree: $worktree" >&2
    exit 1
  fi
elif [ "$mode" != release ] || [[ "$worktree" != /* ]]; then
  echo "worktrees: target must be an existing worktree: $worktree" >&2
  exit 1
fi
registry="$common_dir/lanyard-worktree-ports.tsv"
lock="$registry.lock"
base="${LANYARD_WORKTREE_PORT_BASE:-4327}"
stride="${LANYARD_WORKTREE_PORT_STRIDE:-10}"

trap 'exit 130' INT
trap 'exit 143' TERM

exec {lock_fd}>"$lock"
if ! flock -w 5 "$lock_fd"; then
  echo "worktrees: timed out waiting for the port registry lock" >&2
  exit 1
fi

touch "$registry"

if [ "$mode" = "release" ]; then
  temporary="${registry}.tmp.$$"
  awk -F '\t' -v worktree="$worktree" '$2 != worktree' "$registry" >"$temporary"
  mv "$temporary" "$registry"
  exit 0
fi

slot="$(awk -F '\t' -v worktree="$worktree" '$2 == worktree { print $1; exit }' "$registry")"
if [ -z "$slot" ]; then
  slot=0
  while awk -F '\t' -v slot="$slot" '$1 == slot { found = 1 } END { exit !found }' "$registry"; do
    slot=$((slot + 1))
  done
  printf '%s\t%s\n' "$slot" "$worktree" >>"$registry"
fi

printf 'LANYARD_SITE_PORT=%s\n' "$((base + slot * stride))"
