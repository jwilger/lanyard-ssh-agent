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

@test "bootstrap creates an isolated task worktree and warms safe caches" {
  mkdir -p \
    "$FIXTURE_REPO/.dependencies/cargo" \
    "$FIXTURE_REPO/.dependencies/target" \
    "$FIXTURE_REPO/.direnv" \
    "$FIXTURE_REPO/site/node_modules/example"
  touch \
    "$FIXTURE_REPO/.dependencies/cargo/cache" \
    "$FIXTURE_REPO/.dependencies/target/artifact" \
    "$FIXTURE_REPO/.direnv/nix-profile" \
    "$FIXTURE_REPO/site/node_modules/example/package.json"
  printf '[registry]\ntoken = "do-not-copy"\n' >"$FIXTURE_REPO/.dependencies/cargo/credentials.toml"
  printf 'do-not-copy\n' >"$FIXTURE_REPO/.direnv/generated-secret"
  printf 'do-not-copy\n' >"$FIXTURE_REPO/.env"

  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example

  [ "$status" -eq 0 ]
  worktree="$FIXTURE_REPO/.worktrees/20260720-example"
  [ -d "$worktree" ]
  [ "$(git -C "$worktree" branch --show-current)" = "task/20260720-example" ]
  [ -f "$worktree/.dependencies/cargo/cache" ]
  [ -f "$worktree/.dependencies/target/artifact" ]
  [ -f "$worktree/site/node_modules/example/package.json" ]
  [ ! -e "$worktree/.dependencies/cargo/credentials.toml" ]
  [ ! -e "$worktree/.direnv/generated-secret" ]
  [ ! -e "$worktree/.env" ]
  grep -Eq '^LANYARD_SITE_PORT=[0-9]+$' "$worktree/.env.worktree"

  printf 'worktree-only\n' >"$worktree/.dependencies/cargo/cache"
  [ "$(cat "$FIXTURE_REPO/.dependencies/cargo/cache")" != "worktree-only" ]
}

@test "bootstrap activates the repository hooks before creating a worktree" {
  git -C "$FIXTURE_REPO" config --unset core.hooksPath

  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example

  [ "$status" -eq 0 ]
  [ "$(git -C "$FIXTURE_REPO" config --get core.hooksPath)" = .githooks ]
}

@test "post-checkout warming is idempotent in linked worktrees" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  worktree="$FIXTURE_REPO/.worktrees/20260720-example"
  git_dir="$(git -C "$worktree" rev-parse --path-format=absolute --git-dir)"
  original_environment="$(cat "$worktree/.env.worktree")"

  [ -f "$git_dir/.lanyard-worktree-bootstrapped" ]
  : >"$worktree/.env.worktree"
  run git -C "$worktree" checkout -q -b second-branch
  [ "$status" -eq 0 ]
  [ -f "$git_dir/.lanyard-worktree-bootstrapped" ]
  [ "$(cat "$worktree/.env.worktree")" = "$original_environment" ]
}

@test "bootstrap resumes an existing linked checkout after interruption" {
  "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example
  original_environment="$(cat "$FIXTURE_REPO/.worktrees/20260720-example/.env.worktree")"
  : >"$FIXTURE_REPO/.worktrees/20260720-example/.env.worktree"

  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" 20260720-example

  [ "$status" -eq 0 ]
  [ "$output" = "$FIXTURE_REPO/.worktrees/20260720-example" ]
  [ "$(cat "$FIXTURE_REPO/.worktrees/20260720-example/.env.worktree")" = "$original_environment" ]
}

@test "bootstrap resolves the default HEAD from the invoking linked worktree" {
  git -C "$FIXTURE_REPO" worktree add -q -b source-worktree "$FIXTURE_REPO/source"
  printf 'from-linked-head\n' >"$FIXTURE_REPO/source/linked-marker"
  git -C "$FIXTURE_REPO/source" add linked-marker
  git -C "$FIXTURE_REPO/source" commit -q -m "advance linked source"

  run "$FIXTURE_REPO/source/scripts/worktree-bootstrap.sh" from-linked-head

  [ "$status" -eq 0 ]
  [ -f "$FIXTURE_REPO/.worktrees/from-linked-head/linked-marker" ]
}

@test "bootstrap warms a target revision that predates lifecycle scripts" {
  git -C "$FIXTURE_REPO" checkout -q -b legacy-target
  git -C "$FIXTURE_REPO" rm -qr scripts .githooks
  git -C "$FIXTURE_REPO" commit -q -m "simulate a revision before lifecycle tooling"
  git -C "$FIXTURE_REPO" checkout -q -

  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" legacy-start legacy-target

  [ "$status" -eq 0 ]
  [ -f "$FIXTURE_REPO/.worktrees/legacy-start/.env.worktree" ]
}

@test "bootstrap rejects path traversal" {
  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" ../outside

  [ "$status" -ne 0 ]
  [ ! -e "$FIXTURE_REPO/outside" ]
}

@test "bootstrap rejects an ordinary directory at the target path" {
  mkdir -p "$FIXTURE_REPO/.worktrees/stale-target"

  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" stale-target

  [ "$status" -ne 0 ]
  [[ "$output" == *"target already exists"* ]]
}

@test "bootstrap rejects an existing linked worktree on the wrong branch" {
  git -C "$FIXTURE_REPO" worktree add -q -b wrong-branch \
    "$FIXTURE_REPO/.worktrees/stale-branch"

  run "$FIXTURE_REPO/scripts/worktree-bootstrap.sh" stale-branch

  [ "$status" -ne 0 ]
  [[ "$output" == *"target already exists"* ]]
}

@test "warming rejects another repository before copying caches" {
  foreign="$FIXTURE_REPO/foreign-repository"
  foreign_worktree="$FIXTURE_REPO/foreign-worktree"
  git -C "$FIXTURE_REPO" init -q "$foreign"
  git -C "$foreign" config user.email test@example.invalid
  git -C "$foreign" config user.name "Foreign Worktree Test"
  git -C "$foreign" config commit.gpgsign false
  touch "$foreign/seed"
  git -C "$foreign" add seed
  git -C "$foreign" commit -q -m seed
  git -C "$foreign" worktree add -q -b foreign-linked "$foreign_worktree"
  mkdir -p "$foreign/.dependencies/target"
  touch "$foreign/.dependencies/target/foreign-artifact"

  run "$FIXTURE_REPO/scripts/worktree-warm.sh" "$foreign_worktree"

  [ "$status" -ne 0 ]
  [ ! -e "$foreign_worktree/.dependencies/target/foreign-artifact" ]
}
