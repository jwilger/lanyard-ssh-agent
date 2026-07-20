# Contributing

Lanyard uses trunk-based development with small Conventional Commits integrated
directly into `main`. Create a ticket worktree with `just worktree-create
<ticket>`, do the work under `.worktrees/<ticket>`, and run `nix develop` and
`just check` before publishing the reviewed commit. Behavior changes begin with
a failing black-box or unit test. Architectural decisions belong in `docs/adr/`.

Commits must be SSH-signed, independently reviewed, and pushed only after the
shared gate passes. The protected `main` branch keeps linear history, rejects
force pushes and deletion, and requires CI for contributions made through pull
requests. Direct trunk integration from a reviewed ticket worktree follows the
same review and gate expectations; the local ticket branch is isolation state
and is not published as a remote integration branch.

Tasks are managed by Tiber on its orphan `tasks` branch. Do not edit task data
by hand. Commit messages that finish a task include a `Closes:` trailer with
the Tiber reference.
