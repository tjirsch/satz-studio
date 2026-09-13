# satz-studio

A desktop app for [satz](https://github.com/tjirsch/satz), the tool that compiles an
estate written in Satz to OpenTofu HCL and asks, through the `question` statements of
its packs, what a customer decides. satz-studio runs that interview, edits the estate
map and the estate files through a typed interface that keeps every comment and every
line where it was, runs satz commands, and drives satz through Claude. An answer, a
pack choice or an import id is written by satz's own writer; every other edit is
checked by `satz transpile --check` before it replaces the file. macOS, Linux and
Windows.

Scaffold. The units of the plan land one pull request each.

## What it needs

- **`satz` 0.56.1 or newer.** `MIN_SATZ` in `crates/satz-studio-core/src/satz/binary.rs`
  names the version, the same release the submodule `vendor/satz` is pinned to. Install
  satz with its installer or bring it up to date with `satz self-update`. The app looks
  at the path set in Settings, then on `PATH`, then at `~/.local/bin/satz`. An older
  binary is refused at startup, naming the version found. Nothing runs without satz;
  the app has no mode of its own.
- **The platform's webview.** Windows 10 and 11 ship WebView2. Linux needs
  `webkit2gtk-4.1` (Debian and Ubuntu: `libwebkit2gtk-4.1-0`; Ubuntu 22.04 ships only
  the 4.0 API and does not run it). macOS needs nothing.
- **A Claude credential, for the Chat view only:** `ANTHROPIC_API_KEY`,
  `ANTHROPIC_AUTH_TOKEN`, an `ant auth login` profile, or a key entered in Settings and
  kept in the OS keychain. The app writes no key to disk. Every other view works
  without one.

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
`tests/fixtures`; the tests that drive the `satz` binary (U4 onward) need it installed.

## Repository layout

| path | what it is |
|---|---|
| `Cargo.toml` | the workspace: two crates, `vendor` excluded, every dependency pinned once under `[workspace.dependencies]` |
| `crates/satz-studio-core/` | the headless half: the estate directory, the document layer, the edit primitives and the write discipline, the provider schema and the view model, the satz driver (binary, CLI, `satz mcp` session), the Claude client and agent loop, transcripts, settings, diagnostics. No GUI dependency; tested on every runner |
| `crates/satz-studio/` | the Dioxus 0.7 desktop binary `satz-studio`: the window, the views, the Material 3 Expressive stylesheet |
| `vendor/satz/` | git submodule, pinned to a satz release tag. **The one pin:** satz-core as a path dependency, the presets, the smoke estates and the provider-schema fixture all come from this checkout |
| `vendor/satz-tree-sitter/` | the generated tree-sitter parser for Satz (`src/`), copied from the grammar repository at the commit named in `COMMIT`; `scripts/sync-grammar.sh` refreshes it and `crates/satz-studio-core/build.rs` compiles it |
| `tests/fixtures/` | an estate directory over satz's smoke estates, its `config.toml` pointing into the submodule; a test that writes copies it first |
| `docs/` | [`architecture.md`](docs/architecture.md) and the decision records under [`adr/`](docs/adr/README.md) |
| `scripts/` | the privacy gate (`check-names.sh`), the grammar refresh (`sync-grammar.sh`), what CI runs |

## Privacy

This repository goes public, history included. Nothing shaped like private data enters
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

MIT, see [`LICENSE`](LICENSE).
