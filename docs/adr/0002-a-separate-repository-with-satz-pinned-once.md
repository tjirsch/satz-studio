# 0002 — a separate repository, with satz pinned once

- **Status:** accepted; its rule that `MIN_SATZ` names the submodule's tag is superseded
  by [ADR 0014](0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md), where
  `MIN_SATZ` is the oldest satz the build works with
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

satz-studio depends on satz three ways: it links `satz-core` (the parser and the
fragment pipeline) as a library, it tests against satz's presets, smoke estates and
provider-schema fixture, and it drives the `satz` binary at runtime through `satz mcp`
and the CLI. The question is where the app lives and how those three dependencies are
pinned so they cannot drift apart.

satz's release surface is shaped for one binary. Its `dist-workspace.toml` lists
`cargo:.` as the only member, so cargo-dist ships the root package alone; its targets
are macOS and Linux, with no Windows target; its `profile.dist` sets `panic = "abort"`.
Its interview client is line-mode so that the binary carries no GUI dependency tree. A
desktop app inside that workspace would change every one of those.

## Decision

satz-studio is its own repository and its own Cargo workspace. satz is pinned once: the
git submodule `vendor/satz`, checked out at a satz release tag. From that one checkout
come `satz-core` as a path dependency (`vendor/satz/crates/satz-core`), the presets, the
smoke estates and the schema fixture that `tests/fixtures/smoke/config.toml` points
into. `vendor` is excluded from the workspace so satz's workspace stays its own.

The `satz` binary is required at runtime. `MIN_SATZ` in
`crates/satz-studio-core/src/satz/binary.rs` names the submodule's tag; an older binary
is refused at startup (`SatzError::TooOld`), naming the version found and
`satz self-update`. There is no degraded mode: the JSON shapes and the tool names the
app speaks are that release's. Moving the pin is one change in three places: the
submodule, `MIN_SATZ`, and the recorded reports under `tests/fixtures`.

The repository is public, history included, so the privacy discipline holds from the
first commit: satz's `scripts/check-names.sh` runs as a hook and in CI, and every value
in docs, tests and fixtures is one of satz's documented example values.

## Consequences

- A satz release that changes a report shape or a tool name reaches the app as one
  submodule bump with a `MIN_SATZ` bump; the recorded-report tests say what changed.
- Reports are deserialised with unknown fields ignored and missing required fields
  failing, so a satz release that adds a field needs no bump and one that removes a
  field fails the test that reads it.
- A user with an older satz sees a refusal, not a partly working app. The minimum
  version moves whenever the app depends on a newer field or tool.
- satz has no Windows release. Until it does, the app has no satz to drive on Windows
  and CI builds the binary from the submodule there. That is an item for satz, not
  something this repository works around.
- Two repositories to keep in step; the pin is the only coupling, and it is explicit.
- CI runs formatting, clippy, the tests and a build on ubuntu, macOS and Windows for
  every push and pull request, so the app is held to the pinned satz on each of the
  three systems it ships on.

## Pros and cons of the options

### A — a crate in the satz workspace

- **Good:** one repository, one pull request for a change that spans parser and app,
  no pin at all.
- **Bad:** cargo-dist ships only the root package, so shipping the app means
  restructuring satz's release; the app needs Windows targets satz does not have; the
  GUI dependency tree (webview, GTK, the keyring stores) enters satz's lockfile and
  build; `panic = "abort"` is wrong for an app that must show an error and keep its
  window; satz's reason for a line-mode interview client is undone.

### B — a separate repository with a git dependency on satz-core and copied fixtures

- **Good:** no submodule; cargo resolves the dependency.
- **Bad:** two pins, the git `rev` in `Cargo.toml` and the copied presets, estates and
  schema, which drift the first time one moves without the other. The presets the app
  is tested against become a snapshot nobody refreshes.

### C — vendoring satz-core's source into this repository

- **Good:** no external reference at build time.
- **Bad:** a fork. Every parser change in satz is a manual copy, and the copy is where
  the app's parser and satz's binary start to disagree.

### D — a separate repository with one submodule at a release tag *(chosen)*

- **Good:** the parser the app links, the presets it tests against and the reports it
  records all come from one tag, and `MIN_SATZ` names the same tag for the binary.
- **Bad:** a submodule is one more thing a clone must initialise
  (`--recurse-submodules`), and a bump touches three places by rule.
