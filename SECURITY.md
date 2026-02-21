# Security Policy

## Reporting a vulnerability

If you discover a security issue in G-Type, please report it responsibly:

**Email:** Open a private [GitHub Security Advisory](https://github.com/IntelligenzaArtificiale/g-type/security/advisories/new)

Please **do not** open a public issue for security vulnerabilities.

## What we consider in scope

- API key leakage (logs, URLs, crash dumps).
- Arbitrary code execution via crafted input.
- Privilege escalation.
- Dependency vulnerabilities in direct dependencies.

## What is out of scope

- Denial of service on the local daemon (it's a local tool).
- Attacks requiring physical access to the machine.

## Response timeline

- **Acknowledgment:** within 48 hours.
- **Fix or mitigation:** within 7 days for critical issues.
- **Disclosure:** coordinated after fix is released.

## Current security measures

- API key is sent via `x-goog-api-key` HTTP header, never in URLs.
- No API keys or secrets are written to log output.
- All network traffic uses HTTPS (TLS via `rustls`).
- Binary is stripped in release builds.
