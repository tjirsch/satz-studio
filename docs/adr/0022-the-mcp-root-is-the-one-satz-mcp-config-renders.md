# 0022 — the root of satz-studio's `satz mcp` is the one `satz mcp-config` renders

- **Status:** accepted
- **Date:** 2026-09-24
- **Deciders:** the maintainer

## Context

`satz mcp --root <dir>` confines every path argument a tool takes to `<dir>`: the estate
`satz_open` opens, a pack to review, a report to write, the library `merge-presets`
writes. Two roots were in use for one estate.

- **satz-studio's own session** computed its root as the longest common directory prefix
  of the estate's directory, every directory its `config.toml` names (`yaml_dir`,
  `hcl_dir`, `schema_dir`, `presets_dir`, each of `include_dirs`) and the main file's
  directory (`session_root`, `crates/satz-studio-core/src/satz/session.rs`). A config
  that reaches up widens it; on Unix, two directories that share only `/` make it `/`,
  which confines nothing, and the guard for "no common root" (`SatzError::NoCommonRoot`)
  could fire only between Windows drives.
- **The agent's configuration** carries the root `satz mcp-config` renders: the
  directory holding `config.toml`, canonicalised
  ([satz ADR 0055](https://github.com/tjirsch/satz/blob/main/docs/adr/0055-satz-writes-the-mcp-client-configuration.md)).

So the window and the agent it configured worked under different boundaries, and the
window's could be the whole disk. Two implementations of one boundary is the drift ADR
0020's amendment removed for the configuration block itself.

## Decision

satz-studio asks satz. `EstateSession::open` runs `satz --config <dir> mcp-config <main>
--client claude-code --allow read,write` — the printing run, which writes nothing — reads
the one server of the block it prints (`mcp_config::server`) and starts `satz mcp` with
that server's `--root` (`mcp_config::root`). A block of another shape, or one without a
`--root`, is `SatzError::Printed` quoting it; the session does not open. The common-prefix
computation and `SatzError::NoCommonRoot` are deleted.

## Consequences

- One rule for the root, satz's, for the window and for every client it configures. A
  change to it in satz reaches both with the next satz.
- Opening an estate costs one more short satz run before `satz mcp` starts.
- An estate whose file lies outside its config directory — a `yaml_dir` that reaches up
  or sideways — is refused at `satz_open` with satz's own sentence naming the root,
  exactly as the agent's `satz mcp` refuses it. `tests/fixtures/smoke` is such an estate
  (its `yaml_dir` is satz's submodule), so the tests and the smoke walk open a copy of it
  whose `yaml/` is inside the copy.
- Directories the config names outside the root are still read where satz reads them
  without a path argument — the presets a compile folds in, the schema. The tools that
  take them as a destination refuse: `satz_merge_presets` and `satz_get_presets` on a
  `presets_dir` outside the estate's directory, for the window and the agent alike.

## Pros and cons of the options

### A — ask `satz mcp-config` for the root *(chosen)*

- **Good:** satz owns the boundary it enforces, and the window and the agent share it.
- **Good:** the root is never wider than the estate's own directory.
- **Bad:** an estate laid out across directories it does not contain opens in neither
  the window nor the agent until its files move under its config directory.

### B — keep the common prefix and write it into the agent's configuration with `--file`
or by hand

- **Good:** estates whose config reaches out keep opening.
- **Bad:** a second implementation of satz's boundary, with a root that can be `/`, and
  a configuration that differs from the one `satz mcp-config` writes for everyone else.

### C — compute the config directory in the app

- **Good:** no extra run at open.
- **Bad:** the same rule written twice; the day satz changes it — a canonicalisation, a
  root for a fleet — the two disagree without a test noticing.
