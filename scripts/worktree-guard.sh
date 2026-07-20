#!/usr/bin/env bash
set -euo pipefail

git_dir="$(git rev-parse --path-format=absolute --git-dir)"
common_dir="$(git rev-parse --path-format=absolute --git-common-dir)"

if [ "$git_dir" = "$common_dir" ]; then
  echo "worktrees: commits and pushes from the main checkout are blocked; use 'just worktree-create <ticket>' and work in the linked checkout." >&2
  exit 1
fi
