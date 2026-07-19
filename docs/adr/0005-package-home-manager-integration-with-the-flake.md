# ADR 0005: Package Home Manager integration with the flake

## Status

Accepted

## Context

Lanyard needs one stable agent socket across long-lived terminal multiplexer
sessions while still accepting short-lived forwarded agent sockets from new SSH
logins. A Home Manager user should not have to reproduce the daemon lifecycle,
shell ordering, or SSH client configuration by hand. The daemon currently
accepts one durable fallback through `serve --upstream`; additional agents are
registered dynamically through its control socket.

The daemon binds and owns both its stable agent socket and control socket. A
systemd socket unit would therefore compete with the daemon rather than help
activate it. The runtime and shell integration also rely on
`XDG_RUNTIME_DIR`, while the flake's package remains useful on Darwin.

## Decision

The flake exports `homeManagerModules.default` alongside its packages, apps,
and checks. The module exposes `programs.lanyard-ssh-agent.enable`, `package`,
and a singular `upstream` option that matches the public CLI.

On Linux, the module installs a systemd user service that starts
`lanyard-ssh-agent serve --upstream …`. The daemon—not a socket unit—owns the
stable runtime and control sockets. Bash and Zsh initialization capture an
incoming forwarded `SSH_AUTH_SOCK`, register it without making shell startup
depend on daemon availability, and only then export Lanyard's stable socket
when the corresponding shell is enabled through Home Manager.
Home Manager's SSH client configuration uses `IdentityAgent SSH_AUTH_SOCK`, so
OpenSSH follows that stable environment value.

The package, app, module, and platform-compatible checks are exported for
Apple Silicon Darwin. Linux-only service and shell checks are omitted there;
the module does not claim to manage a Darwin daemon until Lanyard has a native
runtime-directory and launchd design.

## Consequences

- Linux users get a declarative daemon, Bash/Zsh integration, and SSH client
  routing from one module import.
- The forwarded socket is never lost by exporting the stable socket first.
- A missing or stopped daemon does not prevent an interactive shell from
  starting.
- The configured upstream is a literal absolute path; shell variables are not
  expanded inside the systemd unit.
- Darwin continues to receive evaluable package and module outputs without a
  Linux service or an unusable `XDG_RUNTIME_DIR` shell hook.

## Alternatives considered

A systemd socket unit was rejected because the daemon already owns both Unix
sockets. A plural static-agent option was rejected because it would promise a
CLI capability that does not exist. Using Home Manager's generic
`sshAuthSock` module was rejected because it deliberately preserves a
forwarded socket instead of registering it and switching to Lanyard's stable
socket. A launchd agent was deferred because the current runtime contract is
Linux/XDG-specific.

## Revisit when

Revisit if the daemon accepts multiple configured fallback agents, gains a
Darwin runtime-directory contract, supports launchd lifecycle management, or
Home Manager provides a hook that can transform a forwarded agent before
setting its stable `SSH_AUTH_SOCK`.
