# Security policy

Lanyard 1.0 is production-ready within the trust boundary described below.
Please report vulnerabilities privately through GitHub's security advisory
interface rather than a public issue.

The intended trust boundary is a single-owner Linux workstation. The owner,
their local processes, installed tools, environment, and configuration are
trusted. Upstream agents may be absent, locked, stale, slow, or fail requests;
Lanyard must bound those failures and fail closed when none can sign.
