# satz-studio

A desktop app for [satz](https://github.com/tjirsch/satz), the tool that transpiles an
estate written in Satz into HCL code for OpenTofu and asks, through the `question` statements of
its packs, what a customer decides. satz-studio creates an estate with `satz init` or
opens one that exists, runs that interview, edits the estate map and the estate files
through a typed interface that keeps every comment and every line where it was, runs
satz commands, and configures and starts an agentic client on the estate with satz's own
`satz mcp-config` — it runs no model itself. An answer, a pack choice or an import id
is written by satz's own writer; every other edit is checked by `satz transpile --check`
before it replaces the file. macOS on Apple silicon, Linux and Windows on x86_64.

The start screen is the way in, and it has three doors. **Create** runs `satz init` in a
folder that holds no estate yet: satz writes `config.toml`, the directories and the
estate file, deriving the customer's domain, directory id, organization id, billing
account and first administrator from the sign-in of the person signed in on this
computer and saying where each value came from. The form asks for what satz cannot
derive — the customer's short name above all — and offers the derivable values as
overrides that are empty by default. The run streams into a log; when it made an estate,
that estate opens.

**Import** runs `satz import` over infrastructure that already exists: a
`tofu show -json` document, a live organization, folder or project, a Terraform HCL file
or directory. The source decides the shape and the
shape decides the flags, so the form shows the options of the chosen source and no
others. `satz import` imports INTO a project, so a folder that is not an estate yet gets
`satz init` first; the form says which of the two it will do and shows both command lines
before it runs. What satz wrote is read back out of the directory rather than guessed at,
and the estate it wrote opens. Beside the log the result carries satz's own report split
into what it wrote, what it skipped and why, the params it could not derive with its
reason for each, and its warnings — among them the attributes an apply would reset and
the asset types Cloud Asset Inventory does not serve. A state or a Terraform configuration
that names no organization takes it in the form's Organization field (`--organization`),
which satz needs before it writes an estate from it; an HCL import with `--wrap-all`
translates nothing and always needs it.

**Open** walks a folder for every `config.toml` under it and opens one of the estates
beside it.

Once an estate is open the window is ordered by the job, and the rail reads in that
order: **Overview**, which estate this is — its short name, the customer, the customer
id and the organization id as its own file sets them, then the infrastructure it names,
in its own answers — and what it still has to do; **Presets**, for an estate that holds the preset library — an organization's:
every pack satz's pack graph offers, as the estate has it, the packs that need another
hung below it, each switched on and off by satz's own `add-preset` and `remove-preset`;
**Decisions**, the
questions those packs declare and it has not answered; **Estate**, the file
itself — its params, its resource tree, and the interfaces it publishes for other estates
to read, as `satz interfaces` reports them: the core exports every interface
carries, then each interface with the interfaces it uses and its own exports, and —
where its packs declare `entries` statements — its list params: each list param, the
field that keys an entry and the fields an entry carries, each with the pattern its
value must match. Delegated access is given to a team by an entry of the list param
`delegations` of satz's `presets/delegation.satz`, which writes the delegation's
interface with the rest. Five of the six are a path, and each ends with a button to the next:
what the estate is, its presets and workload packs, their questions, its way to the organization, its
documents; Estate stands after them in the rail. **Apply**, its way to the organization — who is signed in, what state the estate is in,
then the checks, the bootstrap, the HCL code, the plan and the apply, in that order;
**Documents**, what goes to people as a file: the decisions sheet, the compliance reports,
and the documents an agent writes, each started in the client Settings names. Agent and Settings sit at the
foot of the rail, with Commands between them. Beside the estate the top bar names sit
reload, "Switch estate" — the Start screen with your estates listed — and "Close estate";
every satz command the app runs is one keystroke away in the commands palette (⌘K,
Ctrl+K on Windows and Linux, or that footer button). The palette also holds "Review a
pack for the preset library": a pack you wrote judged against the library's own bar with
`satz review-preset` — each finding in the Findings drawer at its line — then placed in
the estate's library as a `.local.satz` or handed upstream by a pull request. What a command prints lands in the
log in the form satz writes for a person — text, or markdown for the evidence report;
`json` is a format you pick when you want the file. **Packs**, for every estate, are workload packs: templates taken from a source and adapted
here. satz says what an estate works in — the organization, a delegated folder or a
delegated project — and the window follows it on the same path: an organization's estate
has Presets and Packs, a workload estate Packs alone; Apply leaves the bootstrap to
the organization; the compliance reports are offered where the library's catalogs are. The
Overview names the mode and, for a workload estate, who delegated. Delegated access is
given in Estate → Interfaces: every list param a preset declares is shown with its entries, and
"Add" writes one through `satz add-entry` from a form of the declared fields. A workload
estate is created from the interface folders it was given, on the Start screen. Packs
lists every workload pack the estate uses first, then the other files of `packs_dir`:
the ones the repository wrote itself ("Own pack") and the ones `satz get-pack` fetched.
Choosing a pack source — `satz`, the satz repository's `packs/`, then each source
`config.toml` names, a GitHub repository or a folder — says how each fetched pack stands
against it, with a badge where it needs you ("Update available", "Edited here", "Gone
from source", "Not checked"), and what the source holds beyond them; each is fetched,
updated from the source it came from, kept as your own or added to the estate from its
row. "New pack…" writes a pack of your own into `packs_dir`, adds its `use` line to the
estate and opens it in the application your system gives `.satz` files; Edit opens any
pack the same way. The sources card names them in one line; "Manage" lists them all,
updates the packs fetched from one, removes one no fetched pack names (`satz pack-source
remove`) and adds one (`satz pack-source add`, from a name and a repository with an
optional branch or tag and folder, or a folder you choose); satz writes `config.toml`,
and what it refuses is shown as it says it. What each card means beyond its two lines is
behind its help button.

The Overview is three cards. The identity card says which estate this is — its short
name, the customer, the customer id and the organization id, the estate's other answers,
whom the live calls run as, and the file, state, schema and HCL it has. "Still to do"
lists what the estate owes — no git repository, which
`satz merge-presets` needs before it edits the estate, so an estate outside one cannot
take preset updates (one button runs `git init`, `git add` and a first commit), the
bootstrap unconfirmed, questions unanswered, an estate map that is off or missing, what satz's pack
graph finds wrong with the packs the estate uses, a `use` line without its `when <gate>`,
which switches its pack on whatever the estate answers, no provider
schema, a pack whose notice is open, prerequisites the transpile found undeclared, raw HCL
nobody has reviewed, an HCL directory nothing has planned against. Every row is derived from the estate as it is on
the screen, so it cannot go stale, and the card reads "Nothing left to do" when there is
nothing in it. Documents exports the decisions sheet or the workbook, below.
Nothing about how an estate reached the app is remembered: created, imported and opened
estates show the same list, because the same facts are true of them.

The two documents a customer receives are `satz questions` in two renderings: the
**decisions sheet** — every decision the estate rests on, what it is set to, whether it
was chosen or taken as offered, and what changing it later costs — and the
**workbook**, the same catalog as a spreadsheet the customer fills in and sends back.
**Documents**, the last destination of the rail, carries them as one card: the sheet is
signed before an organization is touched and kept as the record after the apply. Pick a format, choose where
the file goes, and satz writes it (`satz questions <estate> --format <format> --out
<file>`); the app opens it once it is there. The formats are the ones the installed satz
lists in `satz questions --help`, so the picker offers exactly what satz can produce. The
sheet is derived from the estate: after an edit, "Export again" writes the same format
over the same file.

A pack can name one command to run once it is switched on — `satz adopt` for the CIS
org-policy packs, so every policy that is already live is in the state before the first
apply. Switching such a pack on raises that notice as a dialog: it carries the pack's
own sentence and the command, and offers to run it, to say it has been run, or to leave
it for later. The estate says a notice has been dealt with by binding its param, which
is what "I ran it" writes and what `satz adopt --execute --import` writes itself — run
from the dialog, it is satz's own writer under the same guard as an answer: the file is
recorded before, checked after, and put back if either refuses; until
then the notice stands on the Overview, in the Findings drawer, and — for a notice
that holds them up — in front of apply and bootstrap, which satz refuses.

## What it needs

- **`satz` 0.103.0 or newer.** `MIN_SATZ` in `crates/satz-studio-core/src/satz/binary.rs`
  names the oldest satz this build works with. It rises when a satz release breaks the
  app or the app starts using something a later satz introduced, not with every satz
  release. Install satz with its installer or bring it up to date with `satz
  self-update`. The app looks at the path set in Settings, then on `PATH`, then at
  `~/.local/bin/satz` (`%USERPROFILE%\.local\bin\satz.exe` on Windows). An older binary
  is refused at startup, naming the version found; the banner runs `satz self-update` on
  it. With no satz at all, the banner runs satz's own installer — the shell script, or
  `satz-installer.ps1` through PowerShell on Windows — checked against the SHA-256 its
  release publishes, into `~/.local/bin` and without touching your `PATH` or shell
  profile. On Windows `satz self-update` does not install, so "Update satz" runs that
  PowerShell installer into the folder of the satz in use. A satz NEWER than the one the build was tested against runs, and a banner says
  so: both versions, and what satz's release rule makes of the difference — a patch
  changes nothing an estate needs; a minor may mean edits, refusals or a different plan
  — until you dismiss it for that version. Nothing runs without satz; the app has no
  mode of its own.
- **A gcloud sign-in, for the live commands.** `satz init` behind Create,
  `satz import` from a live scope behind Import,
  `whoami`, `report-compliance` and everything else that reads a Google organization run
  as the person signed in with `gcloud auth application-default login`. satz reads that
  sign-in and satz-studio holds none of its own. Signed out, `satz init`
  still writes the directories and `config.toml`, says on its own stderr what it could
  not derive, and writes an estate file only if you stated a customer id.
- **The platform's webview.** Windows 10 and 11 ship WebView2. Linux needs
  `webkit2gtk-4.1` (Debian and Ubuntu: `libwebkit2gtk-4.1-0`; Ubuntu 22.04 ships only
  the 4.0 API and does not run it). macOS needs nothing, and is Apple silicon: there is
  no Intel build (ADR 0016).
- **An agentic client, for the Agent destination.** Anything that speaks the Model
  Context Protocol: Claude Code (`claude`, the default), cowork, Claude Desktop.
  satz-studio calls no model and holds no sign-in — the client brings its own, and
  what the app gives it is this estate's satz server.

  The configuration is satz's own: the Agent destination runs `satz mcp-config
  <estate> --client <client> --allow <ceiling>` and shows what it printed, and beside it
  the ceiling the client's file holds for the estate — the one the agent runs at. The
  ceiling is the one Settings holds for the agent; it bounds the satz server only, since
  an agent with a shell can run satz commands directly, and satz-studio's own session
  runs at `read,write` whatever it is. Saving Settings rewrites no client file.
  **Configure Claude Code** and **Configure Claude Desktop** run it again with
  `--write`, which merges satz's one key into the file that client reads — `.mcp.json`
  beside the estate, or Claude Desktop's own configuration file — and leaves every other
  server in it as it is. satz refuses its own key already there with other arguments, and
  the card shows that entry with **Replace it**, the run that writes the block over it. **Copy** puts the block on
  the clipboard.

  **Open in <client>** starts the command Settings names, in the estate's directory, in
  your terminal. A client that is not installed is said so, by name.

  Every other destination works without a client.

## Build and run from source

Rust 1.88 or newer (`rust-toolchain.toml` selects stable). On Linux the build needs
`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`.

```sh
git clone --recurse-submodules https://github.com/tjirsch/satz-studio.git
cd satz-studio
cargo install dioxus-cli@0.7.10       # or: cargo binstall dioxus-cli@0.7.10
dx serve --package satz-studio        # the app, with hot reload
cargo run -p satz-studio              # the app, plain cargo
cargo test -p satz-studio-core        # the headless tests: no window, no network
```

A clone without `--recurse-submodules` has an empty `vendor/satz` and does not build;
`git submodule update --init` fills it. The tests read the pinned satz checkout and
`tests/fixtures`; the tests that drive the `satz` binary need it installed. Nothing in
the suite needs a network or a sign-in.

## Repository layout

| path | what it is |
|---|---|
| `Cargo.toml` | the workspace: two crates, `vendor` excluded, every dependency pinned once under `[workspace.dependencies]` |
| `release.toml` | cargo-release: the version bump, the commit, the tag `vX.Y.Z` and the push, in one step on `main` |
| `crates/satz-studio-core/` | the headless half: the estate directory, the document layer, the edit primitives and the write discipline, the provider schema and the view model, the satz driver (binary, CLI, `satz mcp` session, the configuration `satz mcp-config` prints), the agentic client it starts, settings, findings. No GUI dependency; tested on every runner |
| `crates/satz-studio/` | the Dioxus 0.7 desktop binary `satz-studio`: the window, the views, the Material 3 Expressive stylesheet |
| `vendor/satz/` | git submodule, pinned to a satz release tag. **The one pin:** satz-core as a path dependency, the presets, the smoke estates and the provider-schema fixture all come from this checkout |
| `vendor/satz-tree-sitter/` | the generated tree-sitter parser for Satz (`src/`), copied from the grammar repository at the commit named in `COMMIT`; `scripts/sync-grammar.sh` refreshes it and `crates/satz-studio-core/build.rs` builds it |
| `tests/fixtures/` | an estate directory over satz's smoke estates, its `config.toml` pointing into the submodule; a test that writes copies it first |
| `docs/` | [`architecture.md`](docs/architecture.md), [`ui.md`](docs/ui.md), [`verification.md`](docs/verification.md) and the decision records under [`adr/`](docs/adr/README.md) |
| `scripts/` | the privacy gate (`check-names.sh`), the satz installer CI runs on Linux and macOS (`install-satz.sh`, the newest satz release verified against its SHA-256 sidecar and held to `MIN_SATZ` or newer; the Windows job does the same in a PowerShell step of `ci.yml`), the verification harness (`e2e.sh`), the grammar refresh (`sync-grammar.sh`), the third-party license texts (`update-third-party-licenses.sh`, which writes `THIRD-PARTY-LICENSES.md` from `Cargo.lock` with `cargo about`; `about.toml` is its allow-list of licenses and its targets, `about.hbs` its layout) |
| `.github/workflows/` | `ci.yml` (once per commit — on a pull request, on the push to `main` that merges it, on a release tag and by hand — and once a day on `main` against the newest satz release, a failed daily run opening an issue: formatting, clippy, tests and a build of the app on Linux, macOS on Apple silicon and Windows; the verification harness and the third-party license texts against `Cargo.lock` on Linux), `release.yml` (the bundles of the three operating systems on a tag) and `names-gate.yml` (the privacy gate over the tree and the commits) |
| `.githooks/` | the pre-commit and commit-msg hooks that run the gate locally |

## Release

`cargo release patch|minor --execute --no-confirm`, run on `main`, bumps the workspace
version in `Cargo.toml`, commits it, tags it `vX.Y.Z` and pushes both; `release.toml`
is that configuration.

The tag runs `.github/workflows/release.yml`: it refuses a tag that is not the
workspace version, then `dx bundle` on each runner and a GitHub release with the
bundles and a `.sha256` sidecar per file. The version in the file names is that same
workspace version. A manual run of the workflow builds the same bundles as workflow
artifacts.

| file | built on |
|---|---|
| `SatzStudio_<tag>_arm64.app.zip`, `SatzStudio_<version>_aarch64.dmg` | `macos-15` |
| `satz-studio_<version>_amd64.deb`, `satz-studio_<version>_x86_64.AppImage` | `ubuntu-24.04` |
| `SatzStudio_<version>_x64.msi` | `windows-2022` |

Every bundle carries `LICENSE`, `NOTICE`, `THIRD-PARTY-LICENSES.md` (the license text of every crate the app is built from), `LICENSE-MaterialSymbols` (the icon font's) and `LICENSE-satz-tree-sitter` (the vendored grammar's): in `SatzStudio.app/Contents/Resources/`,
beside the executable in the MSI's install directory, and in `/usr/lib/SatzStudio/` from
the `.deb` and the AppImage.

The builds are not signed and not notarized. On macOS, Gatekeeper refuses the first
launch with "Apple could not verify “SatzStudio” is free of malware"; System Settings
→ Privacy & Security → "Open Anyway" lets it start, and so does
`xattr -d com.apple.quarantine SatzStudio.app` on the unzipped app. On Windows,
SmartScreen shows "Windows protected your PC"; "More info" → "Run anyway" installs.
The `.deb` and the `.AppImage` prompt nothing. The same bundle is built locally with
`dx bundle --package satz-studio --platform desktop --release` from the repository
root, under `target/dx/satz-studio/bundle/`; `docs/verification.md` is what is checked before a
tag.

The app does not replace itself with a newer release. Once per launch it reads the latest
satz-studio release on GitHub, and asks the satz in use, with `satz self-update
--check-only`, whether a newer satz is released — unless your satz config says
`self_update_frequency = "never"`. The window title shows the app's version and "update
available" for either. Settings is where either is acted on: it opens the new release's
page, where the bundle is installed as above, and runs `satz self-update`. It also says
what each look found, or why it failed, and looks again when asked.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) is the development setup, the checks CI runs and
what a pull request needs. [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) applies to
everyone taking part. [`SECURITY.md`](SECURITY.md) is how to report a vulnerability —
privately, never in a public issue. [`CLAUDE.md`](CLAUDE.md) holds the working rules
for anyone changing this repository, person or agent.

## Privacy

This repository is public, history included. Nothing shaped like private data enters
it: no customer, company or person; no directory id, organization number, billing
account, tenant id, e-mail address or repository path that is not one of satz's
documented example values (`acme`, `example.com`, `C0example`, `123456789012`; the
table is `docs/examples.md` in the satz repository). `scripts/check-names.sh` is satz's
gate: it rejects those shapes in files and in commit messages, and CI runs it on every
pull request and on the push to `main` that merges it. Enable the hooks once per clone:

```sh
git config core.hooksPath .githooks
```

The app keeps no conversation and holds no sign-in, so nothing of that kind is in an
estate or anywhere else it writes.

## Planning

The repository carries no roadmap and no todo file. `README.md` and `docs/` say what the
app does; what is still to do is planned outside the repository.

## License

Apache License 2.0, see [`LICENSE`](LICENSE). [`NOTICE`](NOTICE) names the material
this repository bundles under other terms — the Material Symbols font and the vendored
tree-sitter grammar — ends with satz's own NOTICE verbatim, since satz-core is built
into the app, and travels with a redistribution.
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md) is the license text of every crate
the app is built from, generated from `Cargo.lock` by
`scripts/update-third-party-licenses.sh`. Every release bundle carries those three files
and the license texts of the font and the grammar.
