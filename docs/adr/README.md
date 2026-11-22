# Architecture decision records

One file per decision that was not obvious, in [MADR](https://adr.github.io/madr/)
form: the context, the options weighed with their real trade-offs, what was chosen,
and what it costs.

A decision belongs here when reversing it would be expensive, when a reader of the
code would otherwise ask "why on earth", or when it was a genuine choice between
defensible alternatives. Most changes are none of those and need no record.

## Why they live here rather than in a commit message

A commit message explains one change to whoever reads that commit. A record answers
"why is it like this?" months later, when nobody is looking at the commit and the
alternative has started to look attractive again. satz keeps its records the same way
and for the same reason; this repository follows its conventions so a reader of one
recognizes the other.

## Conventions

- `NNNN-kebab-title.md`, numbered in order, never renumbered. Take the next free number
  from the directory, not from memory.
- The header is **Status**, **Date** and **Deciders**; a decider is named by role
  (`the maintainer`), never by name.
- **Status** is `proposed`, `accepted`, `superseded by ADR-NNNN`, or `rejected`; an
  accepted record that a later one amends or supersedes in part says so after a
  semicolon (`accepted; … superseded by 0021`), as satz's records do. A superseded
  record is not deleted: the reasoning that was right at the time is what is worth
  keeping, and the successor says what changed.
- The sections are Context, Decision, Consequences, and the pros and cons of every
  option, the chosen one marked *(chosen)*. Options are written with their real
  trade-offs, including the one that was chosen; a record whose alternatives are
  strawmen documents nothing.
- Nothing here names a customer, an organization or a person: this repository goes
  public, and the privacy gate treats these files like any other.
- A record that rests on a decision satz made links satz's record
  (`vendor/satz/docs/adr/`) rather than restating it.

## Records

| | decision | status |
|---|---|---|
| [0001](0001-dioxus-desktop-on-the-webview-renderer.md) | Dioxus 0.7 desktop on the webview renderer | accepted; its Material 3 part superseded by 0034 |
| [0002](0002-a-separate-repository-with-satz-pinned-once.md) | a separate repository, with satz pinned once as a submodule and the binary required at a minimum version | accepted; the minimum's rule superseded by 0014, its Windows half by satz's Windows build |
| [0003](0003-the-document-layer-is-the-tree-sitter-grammar.md) | the document layer is the tree-sitter grammar of Satz, vendored and built in; satz-core stays the authority on meaning | accepted |
| [0004](0004-claude-natively-other-providers-adapt-into-its-message-model.md) | Claude natively: the Messages API wire types are the app's message model, and other providers adapt into it | superseded by 0020 |
| [0005](0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md) | tool approval by the MCP annotations satz declares; the capability ceiling stays satz's | accepted; the approval card superseded by 0020, the ceiling the agent's configuration alone since 0021 |
| [0006](0006-apply-and-bootstrap-run-in-the-users-terminal.md) | `apply` and `bootstrap` run in the user's terminal, never with `-auto-approve` | accepted |
| [0007](0007-pack-rows-are-derived-from-the-estate-file.md) | pack rows are derived from the estate file, the questions report and the resolved params; no copied table | superseded by 0018 |
| [0008](0008-transcripts-live-outside-the-estate.md) | transcripts live under the app's data directory, never inside an estate | superseded by 0020 |
| [0009](0009-refusal-fallbacks-are-on-by-default.md) | refusal fallbacks are on by default, off by a Settings switch | superseded by 0020 |
| [0010](0010-claude-code-as-the-subscription-backend.md) | Claude Code as the subscription backend: the installed CLI driven over stdio, the estate's satz MCP server, the app's own approval card | superseded by 0020 |
| [0011](0011-the-licence-is-apache-2-0.md) | the license is Apache 2.0: the express patent grant, contribution terms in §5, and a `NOTICE` for the material bundled under other licenses | accepted |
| [0012](0012-migrate-hands-off-to-the-terminal.md) | `migrate` hands off to the terminal with `apply` and `bootstrap`; `bootstrap --dry-run` is a check that runs in the app | accepted |
| [0013](0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md) | the Claude Code stream log is verbatim, off by default, one file per conversation, and bounded | superseded by 0020 |
| [0014](0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md) | a satz newer than the build runs and is a notice, not a gate; `MIN_SATZ` is the oldest satz the build works with and rises only for a breakage or a use; the app looks for releases of itself and of satz once per launch and says so in the window title, the banner and Settings | accepted; amended 2026-09-27 — the top bar carries no chips |
| [0015](0015-the-packs-view-draws-the-dependency-tree-the-packs-declare.md) | the Packs view draws the dependency tree the packs declare: the edges are the packs' `ask_when`s read with satz-core's parser, a pack others wait on is a block across the grid with its dependents hung below it, and the right-angle connectors are CSS on the tree's own elements | superseded by 0018 |
| [0016](0016-macos-is-apple-silicon-alone.md) | macOS is Apple silicon alone, in CI and in the release: no Intel runner, no Intel bundle; Linux and Windows unchanged on x86_64 | accepted |
| [0017](0017-a-release-is-cargo-release-on-main.md) | a release is `cargo release` on `main` — one step bumps, commits, tags and pushes — and that version-bump commit is the one exception to the pull-request rule | accepted |
| [0018](0018-the-packs-view-shows-satzs-pack-graph.md) | the Packs view shows satz's pack graph: its rows are `satz_packs`, a switch is `satz_add_pack` or `satz_remove_pack`, and the app derives no pack row and no dependency; the tree hangs a pack below the one pack that meets its first requirement | accepted |
| [0019](0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md) | the pack review runs `satz review-pack` through the CLI with the estate's config, since `satz mcp` is confined to the estate's root; a private pack is placed as `<stem>.local.satz` and never over other text; the upstream hand-over is a pull request by hand | accepted; the review is an entry of the commands palette since 0031 |
| [0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md) | satz-studio runs no model: the chat and both its engines are gone, and the Agent destination configures an external client on the open estate — `.mcp.json` or Claude Desktop's own file — and starts that client in the estate's directory | accepted; amended 2026-09-23, the configuration is satz's and the app calls `satz mcp-config` for it; its one ceiling for two sessions superseded by 0021 |
| [0021](0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md) | the Settings ceiling is the external agent's configuration alone; satz-studio's own `satz mcp` runs at `read,write`, what its buttons need; the Agent view shows the ceiling the client's file holds beside the setting and offers Replace where they differ; a save rewrites no file | accepted |
| [0022](0022-the-mcp-root-is-the-one-satz-mcp-config-renders.md) | the root of satz-studio's own `satz mcp` is the `--root` `satz mcp-config` renders for the estate, the directory holding `config.toml`; the common-prefix root is gone | accepted |
| [0023](0023-the-interfaces-tab-reads-and-writes-through-the-cli.md) | the Interfaces tab reads `satz interfaces` and writes with `satz add-project` through the CLI, because the interface plane's MCP tools are paused; the write is satz's own writer inside the delegated-write discipline, and a third tab of Estate rather than a seventh destination | accepted; the write half superseded — satz removed `add-project` (satz ADR 0075) |
| [0024](0024-workload-packs-are-a-tab-of-estate-and-a-workload-estate-has-no-presets-destination.md) | the destination is Presets; workload packs are the fourth tab of Estate, read through the CLI against a source the operator names and never unasked; the `use` line is the document layer's `Edit::AddUse`; a workload estate has no Presets destination | accepted; its two-way rule superseded by 0025 |
| [0025](0025-an-estate-works-in-one-of-three-modes-and-satz-says-which.md) | an estate works in one of three modes — organization, delegated folder, delegated project — read from satz's presets report; an organization has presets and no Workload packs tab, an estate in a delegated place every workload pack, each row saying which delegated values it reads; the mode is a row of the Overview | superseded by 0032 |
| [0026](0026-the-content-uses-the-compact-end-of-the-scale.md) | content sits on the compact end of Material 3's type scale at every window size: a question's title 16 px emphasized, running text 14 or 12 px, `Button` in Expressive's extra-small size (32 px), list rows at least 48 px; no density setting and no width-dependent sizes | superseded by 0034 |
| [0027](0027-documents-is-a-destination-of-its-own.md) | Documents is the seventh primary destination, last in the rail, and holds the decisions sheet as one card where Decisions and the Overview each carried it under its own title; the rail is full at Material 3's seven; the evidence report stays in Checks | accepted |
| [0028](0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md) | Apply is the one destination for the checks and the apply: the sign-in check (`satz_whoami`, online, when the view is entered and on a button), a state line, and the commands in the order they are walked; the compliance commands and the documents an agent writes — started in the configured client on a skill — are Documents'; the rail holds six | accepted |
| [0029](0029-presets-and-packs-are-two-destinations.md) | Presets, for an estate that holds the library, and Packs, for every estate, are two primary destinations; an entry of a list a preset declares is added from a form built from the declaration, through `satz_add_entry`; a workload estate is created from its interface folders (`satz init --from`); the Overview says who delegated; Apply leaves the bootstrap to the organization; `MIN_SATZ` is 0.101.0 | accepted |
| [0030](0030-pack-sources-are-satzs-and-the-window-adds-them-through-satz.md) | the Packs view lists the estate's pack sources from `satz pack-source list`, lists a source's packs with `--source`, adds a source from a dialog through `satz pack-source add` — a chosen folder written relative to the estate's directory — and removes one through `satz pack-source remove`; the preset library is not shown, no satz report carries it; `MIN_SATZ` is 0.103.0 | accepted |
| [0031](0031-the-pack-review-is-in-the-palette-and-a-new-pack-opens-in-the-systems-editor.md) | the pack review is an entry of the commands palette, "Review a pack for the preset library", not a card of Presets; the Packs view lists the packs the estate's `use` lines read before a source is chosen, makes a new pack of the repository's own — written, formatted by `satz fmt`, its `use` line added through `Edit::AddUse` — and opens a pack in the application the system gives `.satz` files; explanations sit behind a help button, and the pack sources are a summary with a Manage dialog | accepted; what opens a pack's file is 0033's |
| [0032](0032-an-estate-is-an-organizations-or-a-workload-estate-and-a-pack-shows-its-declarations.md) | an estate is an organization's or a workload estate, as satz's presets report says it, and the Overview's "Kind" row names it with no sentence; "Delegated by" names each interface folder with what it exports; a listed pack has Details — its params, list params, exports and reads matched against the open estate by `satz packs --estate` — and "Unmet read" where a read is not given; a Reload button, not window focus, reads the list again after an external edit; `MIN_SATZ` is 0.107.0 | accepted |
| [0033](0033-a-pack-opens-in-the-editor-setting-or-the-systems-text-editor.md) | a pack's file opens from Edit and New pack in the Editor setting's command line, the file its last argument, or with the setting empty in the system's text editor (`open -t`, `xdg-open`, `notepad`); the path is resolved before it opens; a program that is not found is not saved; a launch that fails is a toast naming the command and Settings → Editor | accepted |
| [0034](0034-the-window-is-built-from-the-dioxus-component-library.md) | the window is built from the Dioxus component library and takes its look: its components copied in by `dx components add`, its theme, Lucide icons and the Geist typeface; the sidebar folds to icons; `app.css` lays out and never repaints or re-pads a component; what the library lacks is the app's own, built from library parts, in `src/widgets/`; the app is run through `dx`; supersedes the Material 3 part of 0001 and 0026 | accepted |
| [0035](0035-security-is-a-destination-of-hints-on-the-estates-own-resources.md) | Security is a primary destination between Apply and Documents: "Scan" runs `satz scan <estate> --format json`, Checkov over the HCL code the last transpile wrote, and the failed checks are listed by resource with the Satz line that declared it; no sign-in, no catalog, no control numbers; Prowler and a justification are later steps; `MIN_SATZ` is 0.110.1 | accepted |
