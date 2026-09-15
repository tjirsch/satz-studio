# satz-studio — working rules

satz-studio is a desktop app over [satz](https://github.com/tjirsch/satz): it runs the
interview the packs declare, edits the estate map and the values in the estate files
through typed fields, runs satz commands, and drives satz through Claude with the satz
MCP tools as the model's tools. Two crates — `satz-studio-core`, the headless half, and
`satz-studio`, the Dioxus 0.7 window over it — with satz pinned once as the submodule
`vendor/satz`.

Read [`docs/architecture.md`](docs/architecture.md) before changing anything, and the
record under [`docs/adr/`](docs/adr/README.md) for the part you are changing. These are
the rules that apply to every change, whoever or whatever makes it.

## Rules

- **The repository is public, history included.** No customer, company or person is
  named, and no value that is not one of satz's documented example values (`acme`,
  `example.com`, `C0example`, `123456789012`; the table is `docs/examples.md` in the
  satz repository). `scripts/check-names.sh` is the gate: it judges SHAPES — directory
  ids, organisation and project numbers, billing accounts, GUIDs, e-mail addresses,
  unknown domains, checkout paths — in files and in commit messages, and refuses a
  commit whose author or committer is neither the maintainer's identity nor a GitHub
  noreply address. CI runs it (`.github/workflows/names-gate.yml`); the hooks run it
  locally, enabled once per clone with `git config core.hooksPath .githooks`. What the
  gate cannot see is a NAME in prose — that is what review is for.
- **satz is pinned once.** The submodule `vendor/satz` and `MIN_SATZ`
  (`crates/satz-studio-core/src/satz/binary.rs`) name the same version, and a test
  holds them equal. Moving the pin is one pull request that moves all three: the
  submodule, `MIN_SATZ`, and the recorded reports under `tests/fixtures`. There is no
  degraded mode — an older binary is refused at startup, naming the version found.
- **Tests key on content, never on a fixture's line numbers or spacing.** satz
  reformats its own fixtures, so a test that pins a line or a column breaks on the next
  pin bump for no reason. Assert the param, the value, the finding —
  `tests/fixtures/edit/support.rs` has the helpers that read that way.
- **`vendor/satz-tree-sitter/` is generated code.** It is the tree-sitter parser for
  Satz at the commit named in `COMMIT`, compiled by `crates/satz-studio-core/build.rs`.
  Never edit it by hand; `scripts/sync-grammar.sh <checkout>` is what refreshes it, and
  a grammar change is made in the grammar repository first.
- **`cargo fmt -p satz-studio-core -p satz-studio -- --check`, never `--all`.** `--all`
  reaches into `vendor/satz`, which is satz's checkout and satz's formatting.
- **Clippy denies warnings:** `cargo clippy --workspace --all-targets -- -D warnings`.
  A warning is a failure, in CI and before a commit. The rest of the gate is
  `cargo test --workspace --locked`, `bash scripts/e2e.sh` and
  `bash scripts/check-names.sh`. CI runs all of it on Linux, macOS and Windows, on
  every push and pull request.
- **satz owns the estate; the app edits inside a span.** An answer, a pack choice and
  an import id are written by satz's own writer (`satz_interview`, `satz adopt`) on the
  real file, with a `Snapshot` taken first and the bytes written back if the check
  refuses. Every other edit is the document layer's: one value rendered in the style of
  its node and spliced over that node's span, proved by re-parsing and comparing node
  signatures, written to a `.studio-tmp.satz` beside the file, checked with
  `satz transpile --check`, and only then renamed over the original. A sha256 that
  changed on disk is a rollback, never a merge. `docs/architecture.md` §4b is the whole
  discipline; do not add a second way to write a file.
- **Never touch the generated HCL.** `hcl_dir` is satz's output. The app reads the
  estate's `config.toml` to learn where it is and leaves it alone.
- **Fail fast, no silent healing.** A settings file that does not parse is a full-screen
  refusal; a schema directory without a schema is `Missing`; a JSON field satz removed
  fails the deserialisation; a structured payload of a shape the app does not read is a
  failure, never a guess. No defaults invented for broken state.
- **Credentials and transcripts stay out of the estate.** A key lives in the OS
  keychain, a Claude Code login belongs to Claude Code, and transcripts live under the
  app's data directory (ADR 0008).
- **Docs ship with the change.** A change to what the app does updates `README.md` and
  the page under `docs/` that describes it, in the same pull request. Docs say what is,
  in the present tense: no history, no "used to", no rhetoric. Where the code and a doc
  disagree, the code is right and the doc is a bug.
- **A decision that was not obvious gets an ADR.** `docs/adr/`, MADR form, numbered
  from the directory and never renumbered, with the options and their real trade-offs.
  A decision that rests on one satz made links satz's record rather than restating it.
  Most changes need none.
- **Planning lives outside this repository.** There is no roadmap file and no todo file
  here, and nothing in `docs/` is a plan. A pull request carries its own reasoning.
- **One task, one branch, one squash-merged pull request** against `main`; `main` is
  the merged history and nothing is committed to it directly.
- **Release:** the workspace version in `Cargo.toml` is bumped in its own pull request,
  and the tag `vX.Y.Z` comes after it, on `main`. The tag is what builds:
  `.github/workflows/release.yml` runs `dx bundle` on macOS arm64, macOS Intel, Linux
  and Windows and attaches each bundle with a SHA-256 sidecar. The bundles are not
  signed and not notarized; `docs/verification.md` is what is checked before a tag.
- **`Satz` is the language; `satz` is the tool and its repository; `satz-studio` is
  this app.** "an estate written in Satz", "the satz binary", "satz-studio opens it".
- **The window follows Material 3 Expressive.** The tokens and anatomies are
  `crates/satz-studio/assets/css/`; `docs/ui.md` says how far each component follows
  the guidelines and where it departs. A new component states which anatomy it is.
