# Lanyard

Lanyard is a resilient SSH-agent switching proxy. It gives long-lived shells
and terminal multiplexers one stable `SSH_AUTH_SOCK`, then routes each request
to an available forwarded or local agent.

The project is under active construction. The current daemon can safely proxy
one configured upstream; adaptive multi-agent discovery and routing are still
being built.

Documentation: <https://jwilger.github.io/lanyard-ssh-agent/>

Licensed under either Apache-2.0 or MIT, at your option.
