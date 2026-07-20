#!/usr/bin/env bash
set -euo pipefail

name="${1:?usage: worktree-bootstrap.sh <ticket-name> [start-point]}"
start_point="${2:-HEAD}"

case "$name" in
  *[!A-Za-z0-9._-]* | '' | '.' | '..')
    echo "worktrees: ticket name must contain only letters, numbers, '.', '_', and '-'" >&2
    exit 2
    ;;
esac

script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
common_dir="$(git -C "$script_root" rev-parse --path-format=absolute --git-common-dir)"
main="$(cd "$common_dir/.." && pwd -P)"
root="$main/.worktrees"
worktree="$root/$name"
branch="task/$name"

git -C "$main" config core.hooksPath .githooks
mkdir -p "$root"
if [ -d "$worktree" ]; then
  target_git_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-dir 2>/dev/null || true)"
  target_common_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
  target_root="$(git -C "$worktree" rev-parse --path-format=absolute --show-toplevel 2>/dev/null || true)"
  target_branch="$(git -C "$worktree" symbolic-ref --quiet HEAD 2>/dev/null || true)"
  if [ "$target_common_dir" = "$common_dir" ] \
    && [ "$target_git_dir" != "$target_common_dir" ] \
    && [ "$target_root" = "$worktree" ] \
    && [ "$target_branch" = "refs/heads/$branch" ]; then
    "$script_root/scripts/worktree-warm.sh" "$worktree"
    printf '%s\n' "$worktree"
    exit 0
  fi
  echo "worktrees: target already exists: $worktree" >&2
  exit 1
elif [ -e "$worktree" ]; then
  echo "worktrees: target already exists: $worktree" >&2
  exit 1
fi

if git -C "$main" show-ref --verify --quiet "refs/heads/$branch"; then
  git -C "$main" worktree add "$worktree" "$branch"
else
  start_commit="$(git -C "$script_root" rev-parse --verify "$start_point^{commit}")"
  git -C "$main" worktree add -b "$branch" "$worktree" "$start_commit"
fi

"$script_root/scripts/worktree-warm.sh" "$worktree"
printf '%s\n' "$worktree"
