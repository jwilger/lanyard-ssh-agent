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

@test "teardown removes an isolated ticket worktree without deleting its branch" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example

  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" 20260720-example

  [ "$status" -eq 0 ]
  [ ! -e "$FIXTURE_REPO/.worktrees/20260720-example" ]
  git -C "$FIXTURE_REPO" show-ref --verify --quiet refs/heads/task/20260720-example
}

@test "teardown does not depend on lifecycle scripts in the target worktree" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  worktree="$FIXTURE_REPO/.worktrees/20260720-example"
  git -C "$worktree" rm -q scripts/worktree-ports.sh
  git -C "$worktree" commit -q -m "simulate an older target checkout"

  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" 20260720-example

  [ "$status" -eq 0 ]
  [ ! -e "$worktree" ]
}

@test "teardown refuses paths outside the documented worktree root" {
  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" "$FIXTURE_REPO"

  [ "$status" -ne 0 ]
  [[ "$output" == *"outside"* ]]
  [ -d "$FIXTURE_REPO/.git" ]
}

@test "teardown preserves a dirty worktree" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  worktree="$FIXTURE_REPO/.worktrees/20260720-example"
  touch "$worktree/uncommitted"

  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" 20260720-example

  [ "$status" -ne 0 ]
  [[ "$output" == *"dirty"* ]]
  [ -f "$worktree/uncommitted" ]
}

@test "teardown preserves a linked worktree on the wrong branch" {
  worktree="$FIXTURE_REPO/.worktrees/stale-branch"
  git -C "$FIXTURE_REPO" worktree add -q -b wrong-branch "$worktree"

  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" stale-branch

  [ "$status" -ne 0 ]
  [[ "$output" == *"expected task branch"* ]]
  [ -d "$worktree" ]
}

@test "teardown preserves the port lease when Git cannot remove the worktree" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  worktree="$FIXTURE_REPO/.worktrees/20260720-example"
  common_dir="$(git -C "$FIXTURE_REPO" rev-parse --path-format=absolute --git-common-dir)"
  mkdir -p "$FIXTURE_REPO/mock-bin"
  export REAL_GIT="$(command -v git)"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'if [ "${3-}" = worktree ] && [ "${4-}" = remove ]; then exit 86; fi' \
    'exec "$REAL_GIT" "$@"' \
    >"$FIXTURE_REPO/mock-bin/git"
  chmod +x "$FIXTURE_REPO/mock-bin/git"
  export PATH="$FIXTURE_REPO/mock-bin:$PATH"

  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" 20260720-example

  [ "$status" -eq 86 ]
  [ -d "$worktree" ]
  grep -Fq "$(printf '\t')$worktree" "$common_dir/lanyard-worktree-ports.tsv"
}

@test "teardown resumes lease cleanup after the worktree was already removed" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  worktree="$FIXTURE_REPO/.worktrees/20260720-example"
  common_dir="$(git -C "$FIXTURE_REPO" rev-parse --path-format=absolute --git-common-dir)"
  git -C "$FIXTURE_REPO" worktree remove "$worktree"
  grep -Fq "$(printf '\t')$worktree" "$common_dir/lanyard-worktree-ports.tsv"

  run "$FIXTURE_REPO/scripts/worktree-teardown.sh" 20260720-example

  [ "$status" -eq 0 ]
  ! grep -Fq "$(printf '\t')$worktree" "$common_dir/lanyard-worktree-ports.tsv"
}
