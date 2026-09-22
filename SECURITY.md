# Security Policy

## Supported Versions

The current minor version receives security updates. Older ones do not: everyone
is on the current version, and a fix ships as a release.

| Version | Supported          |
| ------- | ------------------ |
| 0.2.x   | :white_check_mark: |
| < 0.2   | :x:                |

## What the app holds

satz-studio holds no credential at all. It calls no model
([ADR 0020](docs/adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)),
so there is no API key, no keychain entry and no conversation kept; the credential the
live satz commands run on is gcloud's Application Default Credentials, which satz reads
and the app never touches. What the app writes outside an estate is the settings file,
which holds no secret, and the one-shot scripts it opens in your terminal. The
configuration it writes for an agent — `.mcp.json` beside the estate, or a block for
Claude Desktop — names the satz binary, the estate's root and the capability ceiling,
and nothing else. The estate's own files are written only through the discipline in
[`docs/architecture.md`](docs/architecture.md): satz's own writer for an answer, a
pack choice and an import id; for every other edit, one value inside its span,
checked by `satz transpile --check` on a temp file and rolled back on a refusal or a
file that changed on disk.

## Reporting a Vulnerability

Please do **not** report security vulnerabilities through public GitHub issues.

Report them by opening a GitHub Security Advisory on this repository
(Security → Report a vulnerability), or by contacting the maintainers directly.

### What to Include

- a description of the vulnerability;
- steps to reproduce it;
- its potential impact;
- the satz-studio version, the operating system, and `satz --version`.

Replace any real estate path, project id or organisation number with an example
value, as anywhere else in this repository.

A report is acknowledged within 48 hours, and fixed as quickly as the issue allows.
