#!/usr/bin/env bats

setup() {
  PROJECT_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd -P)"
}

@test "root guidance requires direct trunk integration without topic publication" {
  for document in README.md CONTRIBUTING.md AGENTS.md; do
    grep -Fq 'git push origin HEAD:main' "$PROJECT_ROOT/$document"
    grep -Fq 'Never publish' "$PROJECT_ROOT/$document"
  done
}
