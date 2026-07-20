# Lanyard

Lanyard is a resilient SSH-agent switching proxy. It gives long-lived shells
and terminal multiplexers one stable `SSH_AUTH_SOCK`, then routes each request
to an available forwarded or local agent.

The daemon discovers same-user OpenSSH agent sockets under `/tmp/ssh-*`, accepts
explicit registrations at runtime, and keeps a configured local agent (normally
1Password) as the final fallback. Identity listings are merged and deduplicated;
read-only requests are tried in priority order and mutation requests fail closed.

```sh
lanyard-ssh-agent serve --upstream "$HOME/.1password/agent.sock"
export SSH_AUTH_SOCK="$(lanyard-ssh-agent socket)"
```

An SSH login can promote its forwarded agent without altering a running
multiplexer session:

```sh
lanyard-ssh-agent register "$SSH_AUTH_SOCK"
lanyard-ssh-agent status --json
lanyard-ssh-agent unregister "$SSH_AUTH_SOCK"
```

Registrations live only for the daemon process. `status --json` reports the
ordered source, path, and current reachability of every candidate. Use
`lanyard-ssh-agent socket --control` to print the stable control-socket path.

Documentation: <https://jwilger.github.io/lanyard-ssh-agent/>

Home Manager users can import `homeManagerModules.default`. On Linux, enabling
`programs.lanyard-ssh-agent` installs the package and systemd user service,
adds forwarded-agent registration to Bash or Zsh when the corresponding shell
is managed by Home Manager, and points OpenSSH at `SSH_AUTH_SOCK`.

## Development

Enter the reproducible environment with `nix develop`. Ticket work happens in
isolated linked worktrees so parallel changes get independent ports and caches
without moving the primary checkout:

```sh
just worktree-create 20260720-example
cd .worktrees/20260720-example
```

Run `just check` before publishing the reviewed commit. Never publish the local
ticket branch. Verify that remote `main` is an ancestor of the reviewed commit,
then integrate that exact commit directly:

```sh
git fetch origin main
git merge-base --is-ancestor origin/main HEAD
git push origin HEAD:main
```

After the commit has been integrated, return to the primary checkout and remove
the linked worktree:

```sh
just worktree-remove 20260720-example
```

See `AGENTS.md` for the complete lifecycle, guard, and cache-isolation policy,
and `CONTRIBUTING.md` for delivery requirements.

Licensed under either Apache-2.0 or MIT, at your option.
