#!/usr/bin/env bash
set -euo pipefail

worktree="${1:-$(git rev-parse --show-toplevel)}"
worktree="$(cd "$worktree" && pwd -P)"
script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
git_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-dir)"
common_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-common-dir)"
target_root="$(git -C "$worktree" rev-parse --path-format=absolute --show-toplevel)"
script_common_dir="$(git -C "$script_root" rev-parse --path-format=absolute --git-common-dir)"
environment_tmp=""
cleanup_environment_tmp() {
  [ -z "$environment_tmp" ] || rm -f "$environment_tmp"
}
trap cleanup_environment_tmp EXIT INT TERM

write_environment() {
  environment_tmp="$worktree/.env.worktree.tmp.$$"
  "$script_root/scripts/worktree-ports.sh" "$worktree" >"$environment_tmp"
  mv "$environment_tmp" "$worktree/.env.worktree"
  environment_tmp=""
}

# Hooks are shared, so remain inert in the primary checkout.
[ "$git_dir" != "$common_dir" ] || exit 0
if [ "$common_dir" != "$script_common_dir" ] || [ "$target_root" != "$worktree" ]; then
  echo "worktrees: target must be a linked worktree from this repository: $worktree" >&2
  exit 1
fi

marker="$git_dir/.lanyard-worktree-bootstrapped"
if [ -f "$marker" ]; then
  environment="$(cat "$worktree/.env.worktree" 2>/dev/null || true)"
  if [[ ! "$environment" =~ ^LANYARD_SITE_PORT=[0-9]+$ ]]; then
    write_environment
  fi
  exit 0
fi

main="$(cd "$common_dir/.." && pwd -P)"
for relative in .dependencies/cargo .dependencies/target site/node_modules; do
  source_dir="$main/$relative"
  destination="$worktree/$relative"
  if [ -d "$source_dir" ]; then
    mkdir -p "$destination"
    if [ "$relative" = .dependencies/cargo ]; then
      rsync -a --exclude=/credentials --exclude=/credentials.toml \
        "$source_dir/" "$destination/"
    else
      rsync -a "$source_dir/" "$destination/"
    fi
  fi
done

write_environment
touch "$marker"
