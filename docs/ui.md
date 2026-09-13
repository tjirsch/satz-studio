# satz-studio UI

The window: what it is built of, how the parts talk, and how far the Material 3
Expressive guidelines are followed. The code is `crates/satz-studio/`; the tokens and
anatomies are `assets/css/`.

## 1. Structure

```
App (src/app.rs)            the stores, the app coroutine, the stylesheets, the theme
└─ Shell (src/shell/)
   └─ EstateHost             one per open estate: owns the estate coroutine
      └─ Frame
         ├─ NavigationRail   destinations, "Open estate" FAB, badges
         ├─ TopBar           estate, runs_as, deployment mode, schema, satz version
         ├─ SatzBanner       satz missing or too old
         ├─ Content          the view of `AppStore.nav`
         ├─ DiagnosticsDrawer
         └─ SnackbarHost
```

`src/main.rs` opens the window (1280×840, at least 900×600) and initialises tracing from
`RUST_LOG`. `App` loads `Settings`; a settings file that does not parse is a full-screen
refusal naming the file, never defaults.

### State

`src/state/mod.rs` holds one `#[derive(Store)]` root, `AppStore`, provided at the root
and read through the generated accessors (`app.nav()`, `app.estate().diagnostics()`),
so a log line re-renders the log and not the rail:

| field | what it is |
|---|---|
| `settings` | the `Settings` as saved; the Settings view edits a draft and saves it whole |
| `satz` | `SatzStatus`: `Unknown` while locating, `Located(SatzBinary)`, `TooOld { found, required }`, `Missing(why)` |
| `root`, `estates`, `discovering` | the folder the Estates view walks and every `config.toml` under it with the estates beside each |
| `open`, `opening` | the `OpenEstate` (its `Arc<EstateSession>`, main file, `runs_as`, deployment mode) and the estate a session is being opened on |
| `nav` | the `View` the rail shows |
| `snackbar` | the toast queue (`VecDeque<Toast>`, three visible) |
| `credential` | what `Credential::resolve` answered |
| `drawer_open` | the diagnostics drawer |
| `estate` | the `EstateStore`: `model`, `cst` (the main file's document tree as read at the last reload — the views slice a value's source text and a line's text from it), `questions`, `interview` (what the last `satz_interview` call returned; `rename_to` is read from it), `diagnostics`, `command_log`, `running`, `last_command`, `outcome`, `loading` — reset when an estate opens or closes |

`DiagnosticSelection(Signal<Option<Diagnostic>>)` is a second context: the drawer sets
it when a row is clicked, the estate views read it.

### Coroutines

Every side effect runs in one of two coroutines; the stores are written from there and
nothing blocks in an event handler.

- **The app coroutine** (`src/state/app_actions.rs`, `AppAction`) locates satz at
  startup (`SatzBinary::locate` with the Settings path), walks `last_root` if there is
  one, and then serves `LocateSatz`, `Discover(folder)` (the walk, the config parse and
  each estate's `deployment_mode` on a blocking thread; the folder becomes
  `Settings.last_root`), `OpenEstate { config, estate }` (`EstateSession::open` with the
  Settings ceiling), `CloseEstate`, `SaveSettings` (the file, then satz again),
  `ResolveCredential` and `StoreKey`.
- **The estate coroutine** (`src/state/estate_actions.rs`, `EstateAction`) is started by
  `EstateHost` with the session and lives as long as the estate is open. On start and on
  `Reload` it calls `satz_questions` over the session, reads the main file, parses it,
  resolves the params and loads the schema on a blocking thread, builds the
  `EstateModel`, and writes the model, the questions and the diagnostics.
  `RunCommand(args)` runs `satz --config <dir> <args…>` in a tokio task and streams its
  lines, ANSI stripped, into `command_log` from a local task, so the loop stays free for
  `CancelCommand`. `RunTool { name, args }` calls one MCP tool and puts its text and its
  structured content in the log. `OpenInTerminal(args)` writes the one-shot script
  (`EstateSession::external_command`) and opens it in the OS terminal. `Close` drops the
  session.

  The estate is written from this coroutine only, every write under
  `EstateSession::write_lock`, verified by `satz transpile --check` and followed by a
  reload:

  - `Answer { subject, value }` is satz's own writer: `Snapshot::take` of the main file,
    `satz_interview {answers: {subject: value}}` (for a `oneof` the value is the chosen
    option's param name), then `Snapshot::verify` through `McpChecker` on the real path.
    A refused tool call wrote nothing and is satz's own sentence in a toast (the brace
    refusal included). A check that refuses restores the bytes, puts its diagnostics in
    the drawer — they stay through the reload that follows — and a toast names the
    first line. `AcceptDefaults` is the same call with `accept_defaults: true`.
  - `CommitEdit(Edit)` is the app's writer: `EditSession::open`, `apply(&[edit])`,
    `Proposed::commit(&McpChecker)`. An edit the document layer refuses (`EditError`)
    is a toast and nothing was written; `Rollback::Check` is diagnostics and a toast as
    above; `Rollback::ChangedOnDisk` is the toast "changed on disk — reloaded".
  - `EnableMap` uncomments the exact comment line `// use "presets/estate-map.satz"`
    (from `scan_uses`: the `Map` row in state `Off`) by splicing the line without its
    `// ` — the one pack line no question gates, so no satz writer activates it —
    under the delegated-write discipline: the bytes recorded, the real path checked, a
    refusal restored. Nothing else is ever uncommented by the app; satz does that on a
    yes.
  - `MergePresets` calls `satz_merge_presets` for a pack row the file has no line for,
    with the outcome in the command log as `RunTool` puts it.

An error from either — a refusal, a missing binary, a function another unit has not
built — is a toast in the snackbar and, where it concerns the estate, a diagnostic in
the drawer or an outcome under the log.

### Views

| view | file | what it does |
|---|---|---|
| Estates | `src/views/estates.rs` | the folder (typed, or picked with the OS dialog), one card per `config.toml` with its estates, each with its deployment mode and an Open button; the open estate is marked and can be closed |
| Commands | `src/views/commands.rs` | the palette (`PALETTE`): `transpile --check`, `transpile`, `questions`, `check-presets`, `iac-roles`, `update-schema`, `hcl-init`, `plan`, `require`, `report-compliance`, `get-presets`, `merge-presets`, and `apply` and `bootstrap` as command lines to copy or open in the terminal; each with its argument fields, the command line as it will run, Run and Cancel, the streamed log with stdout and stderr distinguished, and the session tools `satz_whoami`, `satz_transpile_check`, `satz_questions` as one click each |
| Settings | `src/views/settings.rs` | every `Settings` field as a form: the satz path with the detected version, the MCP ceiling, auto-approve, the provider with base URL and model for the non-Claude ones, the Claude model, effort, fallbacks, transcripts, theme; Save writes the file and locates satz again; the credential card shows where the Claude credential comes from and stores a key in the keychain |
| Gallery | `src/views/gallery.rs` | every component in its variants, light and dark side by side |
| Interview | `src/views/interview.rs` | the questions report one question at a time, unanswered first with a "Show answered" switch: the pack's description when the pack changes, the prompt, the `why`, chips for reversal and blast, a warning banner on a one-way door, the recommendation when it differs from the offer, the field in the shape of the offered value as satz's `parse_answer` types an answer (a switch, a number field, a chip list, a text field that refuses a brace with satz's sentence), a `oneof` as filter chips with the chosen option's `why`; Accept or Answer, Skip, "Accept n defaults"; the progress from `summary`, the complete state, and the `rename_to` card when the last `satz_interview` returned one. Each answer is one `Answer` action |
| Params | `src/views/params.rs` | one row per `ParamRow`, grouped by the asking question's pack (else "estate"): a typed field by `ParamKind` in value mode, or the Satz source in source mode — a row whose value carries a `{param}` or `${…}` opens there, with its parts as chips; the question's `why` as a tooltip, a one-way-door chip, a raw-line toggle showing the line; a commit on Enter, blur, a switch flip or a chip change is `CommitEdit(Edit::ReplaceParam)` with a `TypedValue` in value mode and `TypedValue::Raw` in source mode |
| Map | `src/views/map.rs` | the `PackRow`s: the map row first — Off is a card with "Enable the map" (`EnableMap`), On a chip, Absent the merge-presets remedy — then sections by phase (the phase comment's first line; a line without one joins the section open at that point; every Absent row last under "Not in this file"), one card per gated line with its path, prompt and `why`, a switch bound to the gate (an answer through `Answer`; off keeps the commented line, satz never re-comments one) or one segmented button per `oneof` group over its options, a badge On/Off/Absent, "Run merge-presets" (`MergePresets`) on an Absent row, and the model's "line active, gate false" note inline on its row |
| Resources | `src/views/resources.rs` | two panes: the tree of `ResourceNode`s (an icon per kind, a resource's name, `use` lines as leaves, branches collapsed below depth 2, a chip with the count of required attributes not written) and the selected node's card: kind, type, line, the missing required names, then one row per `AttrRow` — a typed field by `AttrType` (string, number, bool, a list of one of them; everything else and `Unknown` in source mode), locked rows dimmed with the reason (`import-id`, computed, not in the schema), source mode with its chips; a commit is `CommitEdit(Edit::ReplaceValue)`. Without a schema every row is locked and the header carries "Run update-schema". A row clicked in the drawer selects the node at its line |
| Chat | — | a card saying the view is not part of this build; `src/views/mod.rs` and the `match` in `src/shell/mod.rs` are where a view is added |

A view that works on an estate shows a card with a button to Estates while none is
open.

## 2. Material 3 Expressive

The rule for this app is to follow the Material 3 Expressive guidelines and to use the
Material Symbols font. There is no official web implementation of Material 3
Expressive, so the tokens and the anatomies are written in CSS from the specification
at <https://m3.material.io>. What follows is what is followed and what deviates.

### Tokens (`assets/css/tokens.css`)

- **Colour.** The colour roles of <https://m3.material.io/styles/color/roles> as
  `--md-sys-color-*`: primary, secondary, tertiary and error with their `on-`,
  `-container` and `on-…-container`; surface, surface-dim, surface-bright, the five
  surface containers, on-surface, on-surface-variant, surface-variant, outline,
  outline-variant, inverse-surface, inverse-on-surface, inverse-primary, surface-tint,
  scrim, shadow, and the fixed roles. **One committed palette instead of dynamic
  colour:** the Tonal Spot scheme generated from the seed `#3A6EA5` with the Material
  colour utilities (the 2021 colour specification at standard contrast; the file header
  carries the regeneration command). Light is on `:root` and `[data-theme="light"]`,
  dark on `[data-theme="dark"]` and, when the document sets no theme, under
  `prefers-color-scheme: dark`. `Settings.theme` puts `data-theme` on the document
  root; `system` sets nothing.
- **Typography.** The type scale of
  <https://m3.material.io/styles/typography/type-scale-tokens> — display, headline,
  title, body, label, each large, medium and small — as `--md-sys-typescale-*` and as
  `.md-*` classes; the emphasized variant is `.md-emphasized` (weight 700, tracking
  −0.01em). The typeface stack is Roboto, Inter, then the system face; no font is
  bundled for text.
- **Shape.** The corner scale of
  <https://m3.material.io/styles/shape/corner-radius-scale>: none, extra-small 4,
  small 8, medium 12, large 16, large-increased 20, extra-large 28, extra-large-increased
  32, extra-extra-large 48, full.
- **Motion.** The Expressive spring tokens of
  <https://m3.material.io/styles/motion/overview> as durations (spatial fast 350 ms,
  default 500 ms, slow 650 ms; effects fast 150 ms, default 200 ms, slow 300 ms) and
  curves. **Springs are approximated:** a damped spring sampled into a `linear()`
  easing (expressive spatial at damping 0.8 and 0.6, standard spatial at 0.9, effects
  critically damped), with a `cubic-bezier` fallback for an engine without `linear()`.
  No physics runs at animation time.
- **States.** Hover 8 %, focus 10 %, pressed 10 %, dragged 16 %, disabled content 38 %
  and container 12 %, drawn as a `::after` layer with `color-mix()`
  (<https://m3.material.io/foundations/interaction/states>).
- **Elevation.** Levels 0–5 as tinted shadows from the shadow role
  (<https://m3.material.io/styles/elevation/overview>).

### Icons

Material Symbols Rounded, the variable font (`FILL`, `wght`, `GRAD`, `opsz`), bundled
from `assets/fonts/MaterialSymbolsRounded.woff2` with its Apache-2.0 notice beside it,
declared with `font-display: block` so a glyph never shows as its ligature text. `Icon
{ name, filled, size }` renders a glyph by ligature and sets the axes inline; the app
works offline.

### Components (`src/components/`, `assets/css/components.css`)

| component | class | spec page | deviations |
|---|---|---|---|
| `Button` (filled, tonal, outlined, text, elevated) | `.m-button` | <https://m3.material.io/components/buttons/specs> | one size (40 px); the shape morphs from full to medium while pressed |
| `ButtonGroup` (`connected`) | `.m-button-group` | <https://m3.material.io/components/button-groups/specs> | the connected group has fixed 2 px gaps and no pressed-neighbour spread |
| `IconButton` (standard, filled, tonal, outlined; `selected`) | `.m-icon-button` | <https://m3.material.io/components/icon-buttons/specs> | one size (40 px) |
| `Fab` (small, medium, large, extended) | `.m-fab` | <https://m3.material.io/components/floating-action-button/specs> | primary-container colour only; no FAB menu |
| `Chip` (assist, filter, input; `error`) | `.m-chip` | <https://m3.material.io/components/chips/specs> | no suggestion chip; the `error` colouring is the app's own, for a status a chip states |
| `Card` (elevated, filled, outlined) | `.m-card` | <https://m3.material.io/components/cards/specs> | — |
| `TextField` (outlined) | `.m-text-field` | <https://m3.material.io/components/text-fields/specs> | outlined only, no filled variant; no trailing icon, prefix or suffix, no character counter; `onenter` and `onblur` are what a field commits on |
| `Switch` | `.m-switch` | <https://m3.material.io/components/switch/specs> | — |
| `Checkbox` | `.m-checkbox` | <https://m3.material.io/components/checkbox/specs> | no indeterminate state |
| `Radio` | `.m-radio` | <https://m3.material.io/components/radio-button/specs> | — |
| `SegmentedButton` | `.m-segmented` | <https://m3.material.io/components/segmented-buttons/specs> | single-select only |
| `Dialog` | `.m-dialog` | <https://m3.material.io/components/dialogs/specs> | basic dialog only; no full-screen dialog |
| `List`, `ListItem` | `.m-list`, `.m-list-item` | <https://m3.material.io/components/lists/specs> | one- and two-line items; no three-line item, no dividers |
| `Tree`, `TreeItem` | `.m-tree` | — (not a Material 3 component) | a nested list with a disclosure per branch, styled with list-item tokens; a `trailing` slot at the row's end |
| `ChipList` | `.chip-list` (in `views.css`) | — (input chips over a text field) | the values of a list as removable input chips, a field that adds one on Enter or blur, several with commas |
| `TypedField` | — | — (composes `Switch`, `TextField`, `ChipList`) | one field in the shape satz reads a value in (`FieldKind`: bool, number, list, text) over a `Draft`; a brace in a text and a non-number in a number field are refused under the field with satz's sentence, and `oncommit` fires only for a draft without a problem |
| `SourceChips` | `.source-chips` (in `views.css`) | — (assist chips over literal text) | a value as satz reads it: a `{param}` chip with what it resolves to, a `${…}` reference chip, the literal text between |
| `Badge` | `.m-badge` | <https://m3.material.io/components/badges/specs> | — |
| `LinearProgress`, `CircularProgress` | `.m-linear-progress`, `.m-circular-progress` | <https://m3.material.io/components/progress-indicators/specs> | the linear indicator has the Expressive stop indicator; the wavy Expressive variant is not drawn |
| `Tooltip` | `.m-tooltip` | <https://m3.material.io/components/tooltips/specs> | plain tooltip only, below the anchor, no rich tooltip |
| `NavRail`, `NavRailItem` | `.m-nav-rail` | <https://m3.material.io/components/navigation-rail/specs> | collapsed rail only (88 px); no expanded rail, no menu button |
| `TopAppBar` | `.m-top-app-bar` | <https://m3.material.io/components/top-app-bar/specs> | small top app bar only; no scroll behaviour |
| `Snackbar` | `.m-snackbar` | <https://m3.material.io/components/snackbar/specs> | an error toast uses the error-container colours, which the spec does not define for snackbars |
| banner (`SatzBanner`) | `.banner` | — (not a Material 3 component) | an error-container surface with the text and two buttons |

No Material Web Components and no other library are used: the components are Dioxus
components over these classes. The Gallery view is the checklist: every Material
component above in every variant, in the light and the dark scheme side by side; the
three composites (`ChipList`, `TypedField`, `SourceChips`) are seen in the estate views
and their classes live in `views.css`, so `components.css` stays the Material anatomies
alone.

### Shell

- **Navigation rail:** the "Open estate" FAB in the FAB slot; one destination per view
  — Estates `home_storage`, Interview `quiz`, Params `tune`, Map `map`, Resources
  `account_tree`, Commands `terminal`, Chat `chat`, Settings `settings`, Gallery
  `palette`. While an estate is open, Interview carries the count of unanswered
  questions and Resources the count of diagnostics.
- **Top bar:** the estate's file name and directory; chips for `runs_as` ("runs as the
  ADC identity" when the estate impersonates nothing), the deployment mode and the
  schema (provider, version and resource count, or "no schema: run update-schema");
  reload and close; the satz version chip, red when satz is missing or too old; the
  drawer toggle with the diagnostics count.
- **Banner:** while satz is missing or too old, a full-width error banner on every
  view naming the fix (`satz self-update`, or the path in Settings), with "Try again"
  and "Settings".
- **Diagnostics drawer:** a bottom drawer, collapsed to its header, listing the open
  estate's diagnostics grouped Errors, Warnings, Notes, each with `file:line` relative
  to the estate directory and its source; clicking a row sets the selection.
- **Snackbar host:** the toasts, three at most, a notice for five seconds and an error
  for twelve, each dismissable.

## The smoke walk

The manual check of the estate views, over `tests/fixtures/smoke` (satz's own smoke
estate, read from the pinned submodule — copy the estate to a scratch directory before
a step that writes, as the fixture's `config.toml` says) and over a skeleton written by
`satz interview <dir>/yaml/new.satz --create`, which is the estate every pack line
starts commented in. No step needs a credential; every write is checked by
`satz transpile --check` through the estate's `satz mcp` child.

1. **Open.** Estates → the folder → Open. The top bar shows the file, "runs as the ADC
   identity", the schema chip with the provider and its type count; the rail shows the
   count of unanswered questions on Interview.
2. **Answer a question.** Interview → the first open question → Accept (or type a
   value and Answer). The toast says "1 answer written"; the file has one new line in
   `params { }` (`git diff` shows nothing else: no re-emission, comments and alignment
   intact); the question count on the rail drops by one; a diagnostic the compile had
   raised for that param is gone from the drawer.
3. **Enable the map, then a pack.** On the skeleton: Map → "Enable the map" → the line
   `use "presets/estate-map.satz"` is uncommented and the map's questions are open.
   Toggle `use_budget` on → the answer lands as `use_budget = true` and satz
   uncomments `use "presets/organization-budget.satz" when use_budget`; toggle it off
   → `use_budget = false` and the line stays active — the card says so, and the drawer
   carries the model's note "line active, gate false" on that line, shown inline on the
   card.
4. **Edit an attribute.** Resources → a resource → a string row → change it → Enter.
   The toast names the file; the line shows the new value with its `=` column where it
   was; the rest of the file is byte-identical.
5. **Type a brace.** Params → a text row in value mode → type `{x}` → the field turns
   red with "braces interpolate in a Satz string — if `{x}` is what you mean, write
   that param by hand" and nothing is sent; the source-mode toggle is where an
   interpolation is written.
6. **Break a value.** Params → source mode on a row → replace the value with a bare
   name nothing binds → Enter. The toast says "not written — line N: …"; the drawer
   shows the check's diagnostic at that line, source "check", and it stays after the
   reload; clicking it in the drawer selects the node at that line in Resources; the
   file is byte-identical to before.
7. **Missing schema.** Point a copy's `schema_dir` at an empty directory and open it:
   the schema chip is red, Resources locks every row with "no schema" and its header
   carries "Run update-schema", which runs in Commands.
