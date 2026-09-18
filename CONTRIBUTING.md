# Contributing to satz-studio

satz-studio is the desktop app for [satz](https://github.com/tjirsch/satz).
Contributions are welcome. This page says how to report something, how to open a
pull request, and what has to be green before you do.

A bug in the Satz language, in a satz command or in a pack belongs in the
[satz repository](https://github.com/tjirsch/satz). This repository is the app.

## How to contribute

### Reporting bugs

Open an issue with the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md).
Include:

- what you did, what you expected, what happened;
- the operating system, the satz-studio version, and the output of `satz --version`;
- the refusal text or the log line, if there is one.

Estate paths, project ids, organisation numbers and e-mail addresses are private
data. Replace them with satz's example values before you post.

### Suggesting enhancements

Open an issue with the
[feature request template](.github/ISSUE_TEMPLATE/feature_request.md). Say what the
app should do and what you are trying to get done with it.

### Pull requests

1. Fork the repository and branch from `main`.
2. Clone your fork **with the submodule**:
   `git clone --recurse-submodules <your fork>`.
3. Make your change.
4. Run the checks below. All of them.
5. Commit with a message that says what changed, and push your branch.
6. Open a pull request against `main`.

A pull request is a coherent unit of work: a feature, a fix, a refactor. Keep the
change and its documentation together — a change to what the app does updates
`README.md` and the page under `docs/` that describes it, in the same pull request.

## Development setup

1. **Rust stable.** `rust-toolchain.toml` selects it; the workspace needs 1.88 or
   newer.
2. **The submodule.** `vendor/satz` is the one pin: satz-core is a path dependency
   into it, and the presets, the smoke estates and the provider-schema fixture the
   tests read come from the same checkout. A clone without `--recurse-submodules`
   has an empty `vendor/satz` and does not build; `git submodule update --init`
   fills it.
3. **The `satz` binary**, at `MIN_SATZ` or newer (`pub const MIN_SATZ` in
   `crates/satz-studio-core/src/satz/binary.rs`, the oldest satz the app works with,
   at or below the version the submodule is pinned to). `bash scripts/install-satz.sh`
   installs the newest satz release, verified against its SHA-256 sidecar and held to
   that floor, into `~/.local/bin`. An older binary is refused at startup and by the
   tests that drive it. satz has no Windows release: there, build it from the
   submodule with `cargo build --release --manifest-path vendor/satz/Cargo.toml` and
   put `vendor/satz/target/release` on `PATH`.
4. **dioxus-cli 0.7.10**, for running and bundling the app:
   `cargo install dioxus-cli@0.7.10` or `cargo binstall dioxus-cli@0.7.10`. Then
   `dx serve --package satz-studio` runs the app with hot reload and
   `dx bundle --package satz-studio --platform desktop --release`, run from the
   repository root, builds this machine's bundle with `LICENSE` and `NOTICE` in it. `cargo run -p satz-studio` runs the app without it.
5. **The platform's webview.** Windows 10 and 11 ship WebView2. Linux needs
   `webkit2gtk-4.1`, and the build needs
   `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`
   (Ubuntu 22.04 ships only the 4.0 API and does not run the app). macOS needs
   nothing.

`cargo test -p satz-studio-core` runs the headless tests: no window and no
credential. The one live Claude request runs only with `SATZ_STUDIO_LIVE=1` and a
credential, and the one live Claude Code turn only with
`SATZ_STUDIO_LIVE_CLAUDE_CODE=1` and a signed-in `claude`; without those the two
print a note and pass. The Claude Code tests otherwise run against a fake CLI and
need Python 3.

## The checks

CI runs these on every push and pull request, and a pull request merges when they
pass. Run them first:

```sh
cargo fmt -p satz-studio-core -p satz-studio -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
bash scripts/e2e.sh
bash scripts/check-names.sh
```

`cargo fmt` names the two packages. `--all` reaches into `vendor/satz`, which is
satz's own checkout and is formatted by satz. Clippy denies warnings, so a warning
fails the build. `scripts/e2e.sh` is the verification harness
([`docs/verification.md`](docs/verification.md)): the installed satz is at least
`MIN_SATZ`, the end-to-end tests of the core crate pass against it, and the app
builds.

`ci.yml` runs the Linux job and the macOS and Windows jobs on every push and pull
request; `names-gate.yml` runs the privacy gate over the tree and over the commits
the push adds.

## The privacy gate

This repository is public, history included. Nothing shaped like private data
enters it: no customer, company or person; no directory id, organisation or project
number, billing account, tenant id, e-mail address, project path or checkout path
that is not one of satz's documented example values (`acme`, `example.com`,
`C0example`, `123456789012`; the table is `docs/examples.md` in the satz
repository).

`scripts/check-names.sh` is satz's gate, carried here as a copy. It is neutral — it
names nobody — and rejects those shapes in files and in commit messages, local files
if they are ever staged (`CLAUDE.local.md`, `*.local.md`, `.claude/`), and any commit
whose author or committer is neither the maintainer's identity nor a GitHub noreply
address (`<id>+<user>@users.noreply.github.com` — turn on "keep my email address
private" in your GitHub settings).

Enable the hooks once per clone, so the gate runs before a commit exists:

```sh
git config core.hooksPath .githooks
```

## Decisions

Planning lives outside this repository: there is no roadmap file and no todo file
here. A pull request therefore carries its own reasoning — the description says what
the change is for, not only what it does.

A decision that was a genuine choice between defensible alternatives — one a reader
would later ask "why on earth" about, or one that would be expensive to reverse —
gets a record in [`docs/adr/`](docs/adr/README.md), in MADR form: the context, the
options with their real trade-offs, what was chosen and what it costs. Take the next
free number from the directory; records are never renumbered. Most changes need no
record.

## The vendored grammar

`vendor/satz-tree-sitter/` is generated code: the tree-sitter parser for Satz,
copied from the grammar repository at the commit named in `COMMIT` and compiled by
`crates/satz-studio-core/build.rs`. Edit nothing in it. `scripts/sync-grammar.sh
<path-to-grammar-checkout>` is what refreshes it, and a grammar change belongs in
the grammar repository first.

## Licence

The repository is under the Apache License 2.0 ([`LICENSE`](LICENSE)). A contribution
you submit for inclusion in it is licensed under those same terms, by §5 of the licence
itself; there is no separate agreement to sign and none is asked for.

A change that bundles a file from somewhere else adds its line to [`NOTICE`](NOTICE) in
the same pull request, with the licence it comes under and where that licence text sits.
