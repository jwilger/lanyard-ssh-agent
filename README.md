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

Licensed under either Apache-2.0 or MIT, at your option.
