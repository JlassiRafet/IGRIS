# Security policy

## Reporting a vulnerability

Please report it privately: open a security advisory at
<https://github.com/JlassiRafet/IGRIS/security/advisories/new>.
Do not open a public issue for a vulnerability.

Include what you found, how to reproduce it, and what an attacker could do with it. You will get a reply within 7 days.

## Scope

IGRIS runs on your machine and drives other tools. The parts most worth testing:

- the local setup app (`igris setup`), which listens on 127.0.0.1 only and must reject requests from other origins;
- anything that writes or reads credentials;
- the commands IGRIS runs on your behalf.

Supported versions: only the latest release, until 1.0.
