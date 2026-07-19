# Contributing

Lanyard uses trunk-based development with small Conventional Commits on
`main`. Run `nix develop` and `just check` before publishing a change. Behavior
changes begin with a failing black-box or unit test. Architectural decisions
belong in `docs/adr/`.

Commits must be SSH-signed, independently reviewed, and pushed only after the
shared gate passes. The protected `main` branch keeps linear history, rejects
force pushes and deletion, and requires CI for contributions made through pull
requests. Direct trunk commits follow the same review and gate expectations.

Tasks are managed by Tiber on its orphan `tasks` branch. Do not edit task data
by hand. Commit messages that finish a task include a `Closes:` trailer with
the Tiber reference.
