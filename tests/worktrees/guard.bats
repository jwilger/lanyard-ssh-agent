#!/usr/bin/env bats

load test_helper

setup() {
  PROJECT_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd -P)"
  export PROJECT_ROOT
  setup_fixture_repo
  git -C "$FIXTURE_REPO" init -q --bare "$FIXTURE_REPO.remote"
  git -C "$FIXTURE_REPO" remote add origin "$FIXTURE_REPO.remote"
}

teardown() {
  teardown_fixture_repo
  rm -rf "${FIXTURE_REPO:-}.remote"
}

@test "pre-commit rejects the primary checkout and permits a linked worktree" {
  run git -C "$FIXTURE_REPO" commit --allow-empty -m blocked
  [ "$status" -ne 0 ]
  [[ "$output" == *"main checkout"* ]]

  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  run git -C "$FIXTURE_REPO/.worktrees/20260720-example" commit --allow-empty -m allowed
  [ "$status" -eq 0 ]
}

@test "pre-push rejects the primary checkout and permits a linked worktree" {
  run git -C "$FIXTURE_REPO" push origin HEAD:refs/heads/main
  [ "$status" -ne 0 ]
  [[ "$output" == *"main checkout"* ]]

  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  run git -C "$FIXTURE_REPO/.worktrees/20260720-example" push origin HEAD:refs/heads/example
  [ "$status" -eq 0 ]
}
