#!/usr/bin/env bash
set -euo pipefail

reference="${1:?usage: worktree-teardown.sh <ticket-name>}"
script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
common_dir="$(git -C "$script_root" rev-parse --path-format=absolute --git-common-dir)"
main="$(cd "$common_dir/.." && pwd -P)"
root="$main/.worktrees"

case "$reference" in
  */*) candidate="$reference" ;;
  *)
    case "$reference" in
      *[!A-Za-z0-9._-]* | '' | '.' | '..')
        echo "worktrees: ticket name must contain only letters, numbers, '.', '_', and '-'" >&2
        exit 2
        ;;
    esac
    candidate="$root/$reference"
    ;;
esac

if [ ! -d "$candidate" ]; then
  if [[ "$reference" != */* ]]; then
    "$script_root/scripts/worktree-ports.sh" release "$candidate"
    git -C "$main" worktree prune
    exit 0
  fi
  echo "worktrees: worktree does not exist: $candidate" >&2
  exit 1
fi

target="$(cd "$candidate" && pwd -P)"
canonical_root="$root"
if [ "$(dirname "$target")" != "$canonical_root" ]; then
  echo "worktrees: refusing to remove a path outside $canonical_root" >&2
  exit 1
fi

target_git_dir="$(git -C "$target" rev-parse --path-format=absolute --git-dir 2>/dev/null || true)"
target_common_dir="$(git -C "$target" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
target_root="$(git -C "$target" rev-parse --path-format=absolute --show-toplevel 2>/dev/null || true)"
target_branch="$(git -C "$target" symbolic-ref --quiet HEAD 2>/dev/null || true)"
expected_branch="refs/heads/task/$(basename "$target")"
if [ "$target_common_dir" != "$common_dir" ] \
  || [ "$target_git_dir" = "$target_common_dir" ] \
  || [ "$target_root" != "$target" ] \
  || [ "$target_branch" != "$expected_branch" ]; then
  echo "worktrees: refusing to remove a target that is not the expected task branch: $target" >&2
  exit 1
fi

if [ -n "$(git -C "$target" status --porcelain)" ]; then
  echo "worktrees: refusing to remove a dirty worktree: $target" >&2
  exit 1
fi

git -C "$main" worktree remove "$target"
"$script_root/scripts/worktree-ports.sh" release "$target"
git -C "$main" worktree prune
