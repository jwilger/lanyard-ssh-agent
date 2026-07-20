setup_fixture_repo() {
  FIXTURE_REPO="$(mktemp -d)"
  export FIXTURE_REPO

  git -C "$FIXTURE_REPO" init -q
  git -C "$FIXTURE_REPO" config user.email test@example.invalid
  git -C "$FIXTURE_REPO" config user.name "Worktree Test"
  git -C "$FIXTURE_REPO" config commit.gpgsign false

  mkdir -p "$FIXTURE_REPO/scripts" "$FIXTURE_REPO/.githooks"
  cp "$PROJECT_ROOT/scripts/worktree-bootstrap.sh" "$FIXTURE_REPO/scripts/"
  cp "$PROJECT_ROOT/scripts/worktree-ports.sh" "$FIXTURE_REPO/scripts/"
  cp "$PROJECT_ROOT/scripts/worktree-warm.sh" "$FIXTURE_REPO/scripts/"
  cp "$PROJECT_ROOT/scripts/worktree-teardown.sh" "$FIXTURE_REPO/scripts/"
  cp "$PROJECT_ROOT/scripts/worktree-guard.sh" "$FIXTURE_REPO/scripts/"
  cp "$PROJECT_ROOT/.githooks/post-checkout" "$FIXTURE_REPO/.githooks/"
  cp "$PROJECT_ROOT/.githooks/pre-commit" "$FIXTURE_REPO/.githooks/"
  cp "$PROJECT_ROOT/.githooks/pre-push" "$FIXTURE_REPO/.githooks/"
  chmod +x "$FIXTURE_REPO"/scripts/*.sh "$FIXTURE_REPO"/.githooks/*

  printf '.worktrees/\n.env.worktree\n' >"$FIXTURE_REPO/.gitignore"
  git -C "$FIXTURE_REPO" add .
  git -C "$FIXTURE_REPO" commit -q -m seed
  git -C "$FIXTURE_REPO" config core.hooksPath .githooks
}

teardown_fixture_repo() {
  if [ -n "${FIXTURE_REPO:-}" ] && [ -d "$FIXTURE_REPO" ]; then
    git -C "$FIXTURE_REPO" worktree prune >/dev/null 2>&1 || true
    rm -rf "$FIXTURE_REPO"
  fi
}
