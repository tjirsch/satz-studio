# satz-studio

A desktop app for [satz](https://github.com/tjirsch/satz), the tool that compiles an
estate written in Satz to OpenTofu HCL and asks, through the `question` statements of
its packs, what a customer decides. satz-studio runs that interview, edits the estate
map and the estate files through a typed interface that keeps every comment and every
line where it was, runs satz commands, and drives satz through Claude. An answer, a
pack choice or an import id is written by satz's own writer; every other edit is
checked by `satz transpile --check` before it replaces the file. macOS, Linux and
Windows.

The headless core is complete: the document layer, the edit primitives and the write
discipline, the provider schema and the view model, the satz driver, the Claude client
and agent loop, transcripts, settings and diagnostics. The window has its shell and the
Estates, Settings, Commands and Gallery views; the estate views (Interview, Params,
Map, Resources) and the Chat view are built next.

## What it needs

- **`satz` 0.58.1 or newer.** `MIN_SATZ` in `crates/satz-studio-core/src/satz/binary.rs`
  names the version, the same release the submodule `vendor/satz` is pinned to. Install
  satz with its installer or bring it up to date with `satz self-update`. The app looks
  at the path set in Settings, then on `PATH`, then at `~/.local/bin/satz`. An older
  binary is refused at startup, naming the version found. Nothing runs without satz;
  the app has no mode of its own.
- **The platform's webview.** Windows 10 and 11 ship WebView2. Linux needs
  `webkit2gtk-4.1` (Debian and Ubuntu: `libwebkit2gtk-4.1-0`; Ubuntu 22.04 ships only
  the 4.0 API and does not run it). macOS needs nothing.
- **A Claude credential *or* a Claude Code login, for the Chat view only.** Either
  serves it; Settings chooses which engine runs.
  - *A credential:* `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, an `ant auth login`
    profile, or a key entered in Settings and kept in the OS keychain. The app writes no
    key to disk.
  - *A Claude Code login:* the `claude` CLI installed and signed in to a claude.ai
    account (`claude auth login`), which runs the chat on that subscription with no API
    key. The app reads no credential of Claude Code's — only whether it is signed in —
    and gives it the open estate's own satz tools, with the same approval card every
    write passes. Settings shows the account and signs in and out for you.

  Every other view works without either.

## Build and run from source

Rust 1.88 or newer (`rust-toolchain.toml` selects stable). On Linux the build needs
`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`.

```sh
git clone --recurse-submodules https://github.com/tjirsch/satz-studio.git
cd satz-studio
cargo install dioxus-cli@0.7.10       # or: cargo binstall dioxus-cli@0.7.10
dx serve --package satz-studio        # the app, with hot reload
cargo run -p satz-studio              # the app, plain cargo
cargo test -p satz-studio-core        # the headless tests: no window, no credential
```

A clone without `--recurse-submodules` has an empty `vendor/satz` and does not build;
`git submodule update --init` fills it. The tests read the pinned satz checkout and
`tests/fixtures`; the tests that drive the `satz` binary need it installed. The one live
Claude request runs only with `SATZ_STUDIO_LIVE=1` and a credential, and the one live
Claude Code turn only with `SATZ_STUDIO_LIVE_CLAUDE_CODE=1` and a signed-in `claude`;
without those the two print a note and pass. The Claude Code tests otherwise run against
a fake CLI and need Python 3.

## Repository layout

| path | what it is |
|---|---|
| `Cargo.toml` | the workspace: two crates, `vendor` excluded, every dependency pinned once under `[workspace.dependencies]` |
| `crates/satz-studio-core/` | the headless half: the estate directory, the document layer, the edit primitives and the write discipline, the provider schema and the view model, the satz driver (binary, CLI, `satz mcp` session), the Claude client and agent loop, transcripts, settings, diagnostics. No GUI dependency; tested on every runner |
| `crates/satz-studio/` | the Dioxus 0.7 desktop binary `satz-studio`: the window, the views, the Material 3 Expressive stylesheet |
| `vendor/satz/` | git submodule, pinned to a satz release tag. **The one pin:** satz-core as a path dependency, the presets, the smoke estates and the provider-schema fixture all come from this checkout |
| `vendor/satz-tree-sitter/` | the generated tree-sitter parser for Satz (`src/`), copied from the grammar repository at the commit named in `COMMIT`; `scripts/sync-grammar.sh` refreshes it and `crates/satz-studio-core/build.rs` compiles it |
| `tests/fixtures/` | an estate directory over satz's smoke estates, its `config.toml` pointing into the submodule; a test that writes copies it first |
| `docs/` | [`architecture.md`](docs/architecture.md), [`ui.md`](docs/ui.md), [`verification.md`](docs/verification.md) and the decision records under [`adr/`](docs/adr/README.md) |
| `scripts/` | the privacy gate (`check-names.sh`), the satz installer CI runs (`install-satz.sh`, the newest satz release verified against its SHA-256 sidecar and held to `MIN_SATZ` or newer), the verification harness (`e2e.sh`), the grammar refresh (`sync-grammar.sh`) |
| `.github/workflows/` | `ci.yml` (formatting, clippy, tests, the verification harness, a build of the app; Linux, macOS and Windows on every push), `release.yml` (the bundles of the three operating systems on a tag) and `names-gate.yml` (the privacy gate over the tree and the commits) |
| `.githooks/` | the pre-commit and commit-msg hooks that run the gate locally |

## Release

A tag `vX.Y.Z` runs `.github/workflows/release.yml`: `dx bundle` on each runner and a
GitHub release with the bundles and a `.sha256` sidecar per file. The version in the
file names is the workspace version in `Cargo.toml`; the tag is that version with a
`v`. A manual run of the workflow builds the same bundles as workflow artifacts.

| file | built on |
|---|---|
| `SatzStudio_<tag>_arm64.app.zip`, `SatzStudio_<version>_aarch64.dmg` | `macos-15` |
| `SatzStudio_<tag>_x86_64.app.zip`, `SatzStudio_<version>_x86_64.dmg` | `macos-15-intel` |
| `satz-studio_<version>_amd64.deb`, `satz-studio_<version>_x86_64.AppImage` | `ubuntu-24.04` |
| `SatzStudio_<version>_x64.msi` | `windows-2022` |

The builds are not signed and not notarized. On macOS, Gatekeeper refuses the first
launch with "Apple could not verify “SatzStudio” is free of malware"; System Settings
→ Privacy & Security → "Open Anyway" lets it start, and so does
`xattr -d com.apple.quarantine SatzStudio.app` on the unzipped app. On Windows,
SmartScreen shows "Windows protected your PC"; "More info" → "Run anyway" installs.
The `.deb` and the `.AppImage` prompt nothing. The same bundle is built locally with
`dx bundle --package satz-studio --platform desktop --release`, under
`target/dx/satz-studio/bundle/`; `docs/verification.md` is what is checked before a
tag.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) is the development setup, the checks CI runs and
what a pull request needs. [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) applies to
everyone taking part. [`SECURITY.md`](SECURITY.md) is how to report a vulnerability —
privately, never in a public issue. [`CLAUDE.md`](CLAUDE.md) holds the working rules
for anyone changing this repository, person or agent.

## Privacy

This repository is public, history included. Nothing shaped like private data enters
it: no customer, company or person; no directory id, organisation number, billing
account, tenant id, e-mail address or repository path that is not one of satz's
documented example values (`acme`, `example.com`, `C0example`, `123456789012`; the
table is `docs/examples.md` in the satz repository). `scripts/check-names.sh` is satz's
gate: it rejects those shapes in files and in commit messages, and CI runs it on every
push and pull request. Enable the hooks once per clone:

```sh
git config core.hooksPath .githooks
```

Transcripts of the Chat view name projects and ids, so they live under the app's data
directory and never inside an estate.

## Planning

The repository carries no roadmap and no todo file. `README.md` and `docs/` say what the
app does; what is still to do is planned outside the repository.

## Licence

Apache License 2.0, see [`LICENSE`](LICENSE). [`NOTICE`](NOTICE) names the material
this repository bundles under other terms — the Material Symbols font and the vendored
tree-sitter grammar — and travels with a redistribution.
