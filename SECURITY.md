# Security Policy

## Supported Versions

The latest release (and the `latest` Docker images built from `main`) is the
only supported version. Older tags do not receive backported fixes.

## Reporting a Vulnerability

Please report vulnerabilities privately — do not open a public issue.

- Email: **abuse@opn.onl**
- Or use GitHub's private vulnerability reporting on this repository
  ("Security" → "Report a vulnerability"), if available to you.

Include reproduction steps and the affected endpoint or component. You can
expect an acknowledgement within a few days. Please give us reasonable time
to ship a fix before public disclosure.

## Scope notes for self-hosters

- Keep `JWT_SECRET` long and random; the server refuses to start with a weak
  one, but rotate it if it ever leaks (rotating it invalidates all sessions).
- Set `TRUST_PROXY_HEADERS=true` only behind a reverse proxy you control —
  otherwise clients can spoof their IP for rate limiting and analytics.
- `FORCE_HTTPS=true` is expected in production behind TLS termination.
