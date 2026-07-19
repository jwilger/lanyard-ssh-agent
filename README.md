# Lanyard

Lanyard is a resilient SSH-agent switching proxy. It gives long-lived shells
and terminal multiplexers one stable `SSH_AUTH_SOCK`, then routes each request
to an available forwarded or local agent.

The project is under active construction. Its public command surface is
already reserved, but the proxy is not ready for credential use yet.

Documentation: <https://jwilger.github.io/lanyard-ssh-agent/>

Licensed under either Apache-2.0 or MIT, at your option.
