# Security Policy

## Supported Versions

The current minor version receives security updates. Older ones do not: everyone
is on the current version, and a fix ships as a release.

| Version | Supported          |
| ------- | ------------------ |
| 0.2.x   | :white_check_mark: |
| < 0.2   | :x:                |

## What the app holds

satz-studio owns no credential of its own. An API key you enter in Settings goes to
the OS keychain (`satz-studio` / `anthropic-api-key`); the app writes no key to disk
and its `Debug` output never prints a secret. A Claude Code login belongs to Claude
Code: the app reads whether the CLI is signed in, never the credential behind it.
Transcripts of the Chat view name projects, ids and resource addresses, so they live
under the app's data directory and never inside an estate
([ADR 0008](docs/adr/0008-transcripts-live-outside-the-estate.md)). The estate's own
files are written only through the discipline in
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
