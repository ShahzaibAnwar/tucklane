# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x | ✅ |

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report privately through GitHub's
[private vulnerability reporting](https://docs.github.com/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
("Security" tab → "Report a vulnerability") on this repository.

Please include:

* a description of the issue and its impact (e.g. "a crafted junction lets
  Tucklane move files outside the authorized folder"),
* steps to reproduce, using sanitized test files only,
* the Tucklane version and Windows build.

We aim to acknowledge reports within 5 working days and to agree on a
disclosure timeline with you. Credit is given in the release notes unless you
prefer otherwise.

Of particular interest: anything that lets Tucklane read, move, delete or
overwrite files outside the authorized folder, overwrite or lose a file, or
execute file contents.

The design is documented in [docs/SECURITY.md](docs/SECURITY.md).
