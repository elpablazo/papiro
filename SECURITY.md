# Security

Papiro stores personal documents, so security reports are taken seriously.

**Please don't open a public issue for a vulnerability.** Report it privately through GitHub: **Security → Report a vulnerability** on this repository. You'll get an answer as soon as possible, and credit in the fix if you want it.

What's in place today: roles (admin / editor / viewer), private documents, read-only and expiring API tokens, Argon2 password hashing with bounded concurrency, per-IP login throttling, CSRF protection (`SameSite=Strict` + custom header + origin check), SSRF guard on webhooks, an audit log and a strict Content-Security-Policy. There has been no external security audit yet.
