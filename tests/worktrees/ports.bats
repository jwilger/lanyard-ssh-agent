#!/usr/bin/env bats

load test_helper

setup() {
  PROJECT_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd -P)"
  export PROJECT_ROOT
  setup_fixture_repo
}

teardown() {
  teardown_fixture_repo
}

@test "parallel worktrees receive stable distinct site ports" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" first
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" second

  first="$(cat "$FIXTURE_REPO/.worktrees/first/.env.worktree")"
  first_again="$("$FIXTURE_REPO/scripts/worktree-ports.sh" "$FIXTURE_REPO/.worktrees/first")"
  second="$(cat "$FIXTURE_REPO/.worktrees/second/.env.worktree")"

  [ "$first" = "$first_again" ]
  [ "$first" != "$second" ]
}

@test "port allocation rejects the primary checkout" {
  run "$FIXTURE_REPO/scripts/worktree-ports.sh" "$FIXTURE_REPO"

  [ "$status" -ne 0 ]
  [[ "$output" == *"linked worktree"* ]]
}

@test "a terminated allocator exits without acquiring a released lock" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" first
  worktree="$FIXTURE_REPO/.worktrees/first"
  common_dir="$(git -C "$FIXTURE_REPO" rev-parse --path-format=absolute --git-common-dir)"
  registry="$common_dir/lanyard-worktree-ports.tsv"
  "$FIXTURE_REPO/scripts/worktree-ports.sh" release "$worktree"
  printf '%s\n' \
    "trap 'if [[ \$BASH_COMMAND == \"touch \\\"\\\$registry\\\"\" ]]; then trap - DEBUG; kill -TERM \$\$; fi' DEBUG" \
    >"$FIXTURE_REPO/signal-after-lock.bash"

  run env BASH_ENV="$FIXTURE_REPO/signal-after-lock.bash" \
    "$FIXTURE_REPO/scripts/worktree-ports.sh" "$worktree"

  [ "$status" -eq 143 ]
  ! grep -Fq "$(printf '\t')$worktree" "$registry"
}

@test "a crashed allocator leaves no stale lock ownership to reclaim" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" first
  worktree="$FIXTURE_REPO/.worktrees/first"
  common_dir="$(git -C "$FIXTURE_REPO" rev-parse --path-format=absolute --git-common-dir)"
  registry="$common_dir/lanyard-worktree-ports.tsv"
  lock="$registry.lock"
  "$FIXTURE_REPO/scripts/worktree-ports.sh" release "$worktree"
  printf '%s\n' \
    "trap 'if [[ \$BASH_COMMAND == \"touch \\\"\\\$registry\\\"\" ]]; then trap - DEBUG; touch \"$FIXTURE_REPO/lock-held\"; while true; do sleep 1; done; fi' DEBUG" \
    >"$FIXTURE_REPO/crash-after-lock.bash"

  env BASH_ENV="$FIXTURE_REPO/crash-after-lock.bash" \
    "$FIXTURE_REPO/scripts/worktree-ports.sh" "$worktree" &
  allocator_pid=$!
  for _ in {1..100}; do
    [ -e "$FIXTURE_REPO/lock-held" ] && break
    sleep 0.01
  done
  [ -e "$FIXTURE_REPO/lock-held" ]
  kill -KILL "$allocator_pid"
  wait "$allocator_pid" 2>/dev/null || true

  [ ! -L "$lock" ]
  run "$FIXTURE_REPO/scripts/worktree-ports.sh" "$worktree"

  [ "$status" -eq 0 ]
  [[ "$output" == LANYARD_SITE_PORT=* ]]
}
