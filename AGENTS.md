# Agent workflow

All ticket implementation must happen in an isolated linked worktree. Treat the
primary checkout as read-only coordination state; its pre-commit and pre-push
guards intentionally reject publication from there.

Create a checkout for one Tiber ticket at a time:

```sh
just worktree-create 20260720-example
cd .worktrees/20260720-example
```

The create command activates the repository's tracked hooks before it creates
or resumes the linked checkout.

The lifecycle hook warms independent Rust and npm build caches from the primary
checkout. Nix reuses its immutable global store; generated `.direnv` state is
never copied. The hook does not copy `.env`, Cargo credentials, or other
secrets. Each linked checkout receives an ignored `.env.worktree` with an
isolated site test port; direnv loads it automatically.

Run the relevant checks and commit from the linked checkout. Never publish the
local ticket branch. After review, verify that the remote trunk is an ancestor
of `HEAD`, then integrate the exact commit directly:

```sh
git fetch origin main
git merge-base --is-ancestor origin/main HEAD
git push origin HEAD:main
```

Never create a pull request unless the user explicitly requests one. After the
commit is integrated and the checkout is clean, remove it from the primary
checkout:

```sh
just worktree-remove 20260720-example
```

Do not bypass the worktree guards. If teardown refuses a dirty checkout,
preserve or integrate its changes before trying again.
