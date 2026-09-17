# satz-studio UI

The window: what it is built of, how the parts talk, and how far the Material 3
Expressive guidelines are followed. The code is `crates/satz-studio/`; the tokens and
anatomies are `assets/css/`.

## 1. Structure

```
App (src/app.rs)            the stores, the app coroutine, the stylesheets, the theme,
│                           the ⌘K listener on the window
└─ Shell (src/shell/)
   └─ EstateHost             one per open estate: owns the estate coroutine
      └─ Frame
         ├─ NavigationRail   six primary destinations, Chat and Settings at the foot
         ├─ TopBar           estate, runs_as, satz version, palette, reload, switch
         ├─ SatzBanner       satz missing, too old or not running; a notice while newer than the build
         ├─ Content          the view of `AppStore.nav`
         ├─ DiagnosticsDrawer
         ├─ CommandPalette   over the window while `AppStore.palette_open`
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
| `satz` | `SatzStatus`: `Unknown` while locating, `Located(SatzBinary)` — at the build's satz or newer, which runs; `satz_notice` reads a newer one for the banner —, `TooOld { path, found, required }`, `Missing(why)` (nothing found), `Unusable(why)` (found, and it does not run or prints no version). `binary()` answers for `Located` alone, which is what every way of opening an estate asks |
| `update` | the `UpdateStore`: the `satz self-update` run — `log`, `running`, `command`, `outcome` — and `found`, what the last `--check-only` run found (the launch check or one asked for), kept for the session and cleared when an update installs; `not_checked` says why no check ran at launch |
| `studio_look` | the `StudioLookStore`: `looking`, and `outcome` — the latest satz-studio release compared with this build, or why the look failed — made once at launch and again when asked, kept for the session |
| `install` | the `InstallStore`: the run of satz's installer — `log`, `running`, `command`, `outcome`, reset when a run starts; the download and the SHA-256 check lead the log |
| `root`, `estates`, `discovering` | the folder the Start screen walks and every `config.toml` under it with the estates beside each |
| `open`, `opening` | the `OpenEstate` (its `Arc<EstateSession>`, main file, `runs_as`, deployment mode) and the estate a session is being opened on |
| `nav` | the `View` the window stands on |
| `door` | the `Door` of the Start screen the pane below its door row belongs to: `Create`, `Import` or `Open` |
| `palette_open` | the commands palette is over the window |
| `snackbar` | the toast queue (`VecDeque<Toast>`, three visible) |
| `credential` | what `Credential::resolve` answered |
| `drawer_open` | the diagnostics drawer |
| `create` | the `CreateStore`: the one `satz init` run behind the Create door — `log`, `running`, `command`, `outcome`, reset when a run starts. What `init` derives from the credentials is in these lines while the window shows them and in the estate satz wrote; it reaches no file of the app's own |
| `estate` | the `EstateStore`: `model`, `cst` (the main file's document tree as read at the last reload — the views slice a value's source text and a line's text from it), `questions`, `interview` (what the last `satz_interview` call returned; `rename_to` is read from it), `diagnostics`, `command_log`, `running`, `last_command`, `outcome`, `loading`, `hcl` (the `HclState` of `hcl_dir`: whether `main.tf` is there and whether it has been initialised), `work_tree` (whether git holds the estate file's directory in a work tree — `None` until the first reload has asked) — reset when an estate opens or closes |

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
  Settings ceiling), `CloseEstate`, `CreateEstate { dir, options }`, `CancelCreate`,
  `SaveSettings` (the file, then satz again), `ResolveCredential` and `StoreKey`.
  At startup, after locating satz, it looks for releases once ([ADR
  0014](adr/0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md)): the latest
  satz-studio release into `studio_look`, and `satz self-update --check-only` on the satz
  in use into `update.found` — unless the operator's `~/.config/satz/satz.toml` says
  `self_update_frequency = "never"`, which puts the reason in `update.not_checked`. A look
  started at launch raises no toast when it fails; a run the operator asks for does.
  `UpdateSatz` refuses a second run while one is running. `DismissSatzNotice(version)`
  writes the version into `Settings.dismissed_satz`, which hides the banner's notice for
  that satz version and changes nothing else. `LookForStudioUpdate` reads the latest
  satz-studio release again from a task of its own, one look at a time. `InstallSatz` runs satz's installer
  (`state/install.rs` over `satz::install`) while the status is `Missing` and Settings
  name no satz path — the installer writes `~/.local/bin/satz`, which the search does not
  look at while a path is set — streams it into `install.log`, locates satz again, and
  calls the install done only if satz is then found; `CancelInstall` stops it. On Windows
  `InstallSatz` is a toast saying satz publishes no Windows build.
  `OpenEstate` puts the window on `View::Overview` when the session opens and
  `CloseEstate` puts it back on `View::Start` — the window has nowhere to stand without
  an estate, so closing one IS switching estates.
  `CreateEstate` checks the target, runs `satz init` in it through `SatzCli::run_in`
  from a task so the loop stays free for `CancelCreate`, streams the lines into
  `create.log`, and then reads what the run left with `init::created`: one estate is
  opened, none is the outcome saying `init` had no customer id to name a file after, and
  a non-zero exit carries satz's own last line rather than a status number. `save_settings` is the one saver: the Chat view's
  "Use Claude Code" awaits it directly rather than writing the file itself, and builds
  its engine again only when the save returned.
- **The estate coroutine** (`src/state/estate_actions.rs`, `EstateAction`) is started by
  `EstateHost` with the session and lives as long as the estate is open. On start and on
  `Reload` it reads `hcl_dir` into `estate.hcl`, asks git whether the estate file's
  directory is in a work tree (`WorkTree::read`) into `estate.work_tree`, calls
  `satz_questions` over the session,
  reads the main file, parses it, resolves the params and loads the schema on a blocking
  thread, builds the `EstateModel`, and writes the model, the questions and the
  diagnostics. A reload that built a model then runs `satz_transpile_check` and adds
  what the compile found — a prerequisite the estate does not declare, a required
  argument the provider wants, raw HCL nobody has reviewed — as diagnostics of source
  `check`. It is skipped when the front end already refused (the refusal has said why,
  and the check would say it twice) and when the reload follows a write, whose own check
  has just run and whose findings are carried in.
  `RunCommand(args)` runs `satz --config <dir> <args…>` in a tokio task and streams its
  lines, ANSI stripped, into `command_log` from a local task, so the loop stays free for
  `CancelCommand`. `RunTool { name, args }` calls one MCP tool and puts its text and its
  structured content in the log. `OpenInTerminal(args)` writes the one-shot script
  (`EstateSession::external_command`) and opens it in the OS terminal. `Close` drops the
  session.
  `InitRepository` is the Overview's repository row pressed: `git init -b main`,
  `git add -A` and one commit naming the estate (`git::init_steps`), run in the estate
  directory one after the other through `git::run`, streamed into `command_log` like a
  command and cancelled like one, stopping at the first step git refuses with git's own
  last line as the outcome. It is refused before git runs when the estate directory has
  no `.gitignore` — `git add -A` would commit the Terraform state and the provider schema
  with the estate — and when the estate file is outside the estate directory; when git
  already holds the directory it runs nothing. The commit takes git's configured
  identity: the app sets none, so without one the refusal in the log is git's. The run
  ends by asking git again, so the row goes when the repository is there.

  The estate is written from this coroutine only, every write under
  `EstateSession::write_lock`, verified by `satz transpile --check` and followed by a
  reload:

  - `Answer { subject, value }` is satz's own writer: `Snapshot::take` of the main file,
    `satz_interview {answers: {subject: value}}` (for a `oneof` the value is the chosen
    option's param name), then `Snapshot::verify` through `McpChecker` on the real path.
    A refused tool call wrote nothing and is satz's own sentence in a toast (the brace
    refusal included). A check that refuses restores the bytes, puts its diagnostics in
    the drawer — they stay through the reload that follows — and a toast names the
    first line; a check that passes puts the findings it reported there instead, so a
    warning shows at its line beside the write that landed. `AcceptDefaults` is the
    same call with `accept_defaults: true`.
  - `CommitEdit(Edit)` is the app's writer: `EditSession::open`, `apply(&[edit])`,
    `Proposed::commit(&McpChecker)`. An edit the document layer refuses (`EditError`)
    is a toast and nothing was written; `Rollback::Check` is diagnostics and a toast as
    above; `Rollback::ChangedOnDisk` is the toast "changed on disk — reloaded". A commit
    that lands carries the check's own findings into the drawer, as an answer does.
  - `EnableMap` uncomments the exact comment line `// use "presets/estate-map.satz"`
    (from `scan_uses`: the `Map` row in state `Off`) by splicing the line without its
    `// ` — the one pack line no question gates, so no satz writer activates it —
    under the delegated-write discipline: the bytes recorded, the real path checked, a
    refusal restored. Nothing else is ever uncommented by the app; satz does that on a
    yes.
  - `WritePrerequisites` is the same delegated write over `satz_update_prerequisites
    {report_only: false}`: satz works out offline which roles the IaC service account
    lacks and which APIs the infra project does not enable, and writes both into the
    estate. `delegated_write` is the one implementation the two share.
  - `MergePresets` calls `satz_merge_presets` for a pack row the file has no line for,
    with the outcome in the command log as `RunTool` puts it.

An error from either — a refusal, a missing binary, a function another unit has not
built — is a toast in the snackbar and, where it concerns the estate, a diagnostic in
the drawer or an outcome under the log.

### Views

The window is ordered by the job, not by what was built when. With an estate open the
rail reads **Overview, Packs, Decisions, Estate, Checks, Deploy**, then Chat and
Settings at its foot; with none open it carries Settings alone and the window stands on
the Start screen. Packs stands before Decisions because a pack is what DECLARES a
question: `merge-presets`, which brings in the pack lines a library gained, is in Packs,
so the questions Decisions lists are what Packs has just let in. An estate created here
lands on Decisions rather than Overview — a skeleton has nothing else yet — and every
other door lands on Overview; the rail's order is the same for all of them.

**Decisions and Packs/Estate are not duplicates, and this is the rule that says so:**
Decisions is the WORKLIST — what the estate has not decided yet, one question at a time,
with what each costs to change later. Packs and Estate are the STATE — which packs the
file runs, which params it binds, which resources it declares. A param that answers a
question is editable in both, on purpose: the question is where the choice is explained,
the file is where it lives. It is the same rule as "one fact, one place" — a param that
GATES a pack line shows in Packs and not in Estate, because there it is a pack choice
and not a value.

| view | file | what it does |
|---|---|---|
| Start | `src/views/estates.rs` | the way in: a row of three door cards (`state::Door`) over the pane the chosen door opens. **Open** is the folder (typed, or picked with the OS dialog), one card per `config.toml` with its estates, each with its deployment mode and an Open button; the open estate is marked and can be closed. It is where the window stands with no estate open, and where closing one returns it |
| Create | `src/views/create.rs` | the **Create** door: the folder the new estate goes in, picked or typed, refused while it is not there or already holds a `config.toml`; the fields satz cannot derive (customer shortname, default region, the Terraform tool as a segmented button, the Google provider set as a switch, extra providers as a chip list); the customer id and the billing account as overrides, empty by default and each saying which live call answers it when it is left blank; the command line as it will run; Create and Cancel; and the run log with stdout and stderr distinguished, a line saying what satz derives is printed there and kept nowhere else, and the outcome chip — satz's own last line when the run refused |
| Import | `src/views/import.rs` | the **Import** door: the folder the import runs in, picked or typed, with a line saying which of the two things will happen — `satz import` alone, or `satz init` first because the folder holds no `config.toml`; the source as a segmented button over the four shapes (state document, live scope, Terraform HCL, legacy YAML) with the source field and its picker below, each shape saying in its supporting line what it reads — the state one that it is `tofu show -json` output and not a raw `.tfstate`; then the flags of that shape alone (only/exclude/all, the collision rule, the customer shortname, the output file and `--verbose` for state and live; wrap-all for HCL; kind, gate and fork for YAML); the Terraform tool and the provider-schema switch when the init half runs; the command line, or both of them, as they will run; Import and Cancel; and, beside the log, satz's report split into what it wrote, what it skipped, the params it could not derive and its warnings, with "Check it compiles" running `satz_transpile_check` on the estate that opened |
| Overview | `src/views/overview.rs` | the identity card first — which estate this is, by its own answers: `identity()` reads the rows the `estate_core` pack declares out of the questions report, in a written order with the customer at its head (Customer, Short name, Customer ID, Organisation ID, Domain, then the infrastructure, the IaC identity, the engine and the region), a core subject the order does not name under a label made from its own name, an unanswered one saying so rather than showing the pack's default, and `deployment_mode` left to the State row below it; then the file, directory, state, schema and HCL rows, and a chip counting what is unanswered. Below it "Still to do": what the estate owes, derived on every render by `owed()` over `Facts` — the estate not in a git repository (so `satz merge-presets` refuses and no preset update can land; git's own words, and "Create the repository" running `InitRepository`) or git not installed (said, with nothing to press), day 0 unconfirmed, questions unanswered, the map off or absent, pack choices with no line, no provider schema, prerequisites the compile found undeclared, raw HCL without a `trust` reason, an HCL directory that has not been compiled or not been initialised — each row with what to do about it: a destination, a command run here, a terminal hand-off, or `merge-presets`. That card is replaced by "Nothing left to do" when the list is empty, and, once something has run, the shared command log follows |
| Decisions | `src/views/interview.rs` | the questions report one question at a time, unanswered first with a "Show answered" switch: the pack's description when the pack changes, the prompt, the `why`, chips for reversal and blast, a warning banner on a one-way door, the recommendation when it differs from the offer, the field in the shape `answer_kind` gives the answer — the offered value's, as satz's `parse_answer` reads an answer, and for a question that offers nothing the shape its param has in the fold, which for a param a pack declares `[]` is a list — as a switch, a number field, a chip list or a text field that refuses a brace with satz's sentence; a chip list sends the answer when a chip is added or removed, several values typed at once with commas, and an empty field is never sent for a question that offers nothing; a question that offers nothing while the estate's params did not resolve shows no field and says the shape is not known, a `oneof` as filter chips with the chosen option's `why`; Accept or Answer, then the next question; Back to the question last left — the one just answered included, switching "Show answered" on when it is answered — and Skip, which reads Next on a question that is answered or not asked; "Accept n defaults"; with "Show answered" on, the list-detail layout: every question beside the card, its prompt, subject and answer, the one on the card selected, a click opening it, the list scrolling on its own and moving below the card in a narrow window; the progress from `summary`, the complete state, and the `rename_to` card when the last `satz_interview` returned one. Each answer is one `Answer` action |
| Packs | `src/views/map.rs` | the `PackRow`s: the map row first — Off is a card with "Enable the map" (`EnableMap`), On a chip, Absent the merge-presets remedy — then sections by phase (the phase comment's first line; a line without one joins the section open at that point; every Absent row last under "Not in this file"), one card per gated line with its path, prompt and `why`, a switch bound to the gate (an answer through `Answer`; off keeps the commented line, satz never re-comments one) or one segmented button per `oneof` group over its options, a badge On/Off/Absent, "Run merge-presets" (`MergePresets`) on an Absent row and once for the whole library beside the map chip, and the model's "line active, gate false" note inline on its row. The `PackEdge`s make a tree of the same cards (`sections(rows, edges)`, a pure function): a card that no other waits on and that waits on none stays a cell of its section's grid; a card others wait on is a block across the whole grid row — a `ConnectorTree` with the card at the top and, below it, the cards asked only when it is on, in the file's order, each hung by a right-angle connector and recursing for a card that others wait on in turn (Security Command Center → notifications → mail and SIEM is three levels). A child hangs under its parent wherever its own line stands, and leaves its phase: a section left with nothing is not shown, and a child whose phase is not its parent's carries that phase's first line as a caption above its card. A connector is solid for "asked only when"; where the child also defaults to its parent by reference its horizontal part is dashed and the caption reads "follows `<gate>`"; where the child's pack is in the estate — line on, gate on — while its parent's is not, the connector and the trunk leading to it are in the error colour and the caption reads "on while `<gate>` is off". A `oneof` is its one group card in the tree as anywhere else, and a card waiting on one of its options hangs from the group |
| Estate | `src/views/estate.rs` | the main file in two tabs, `ParamsPane` and `ResourcesPane`, with the counts on each. A diagnostic chosen in the drawer opens the Resources tab, where its line is |
| Estate · Params | `src/views/params.rs` | one row per `ParamRow`, grouped by the asking question's pack (else "estate"): a typed field by `ParamKind` in value mode, or the Satz source in source mode — a row whose value carries a `{param}` or `${…}` opens there, with its parts as chips; the question's `why` as a tooltip, a one-way-door chip, a raw-line toggle showing the line; a commit on Enter, blur, a switch flip or a chip change is `CommitEdit(Edit::ReplaceParam)` with a `TypedValue` in value mode and `TypedValue::Raw` in source mode |
| Estate · Resources | `src/views/resources.rs` | two panes: the tree of `ResourceNode`s (an icon per kind, a resource's name, `use` lines as leaves, branches collapsed below depth 2, a chip with the count of required attributes not written) and the selected node's card: kind, type, line, the missing required names, then one row per `AttrRow` — a typed field by `AttrType` (string, number, bool, a list of one of them; everything else and `Unknown` in source mode), locked rows dimmed with the reason (`import-id`, computed, not in the schema), source mode with its chips; a commit is `CommitEdit(Edit::ReplaceValue)`. Without a schema every row is locked and the header carries "Run update-schema". A row clicked in the drawer selects the node at its line |
| Checks | `src/views/checks.rs` | what judges the estate: the `CHECKS` deck — `transpile --check`, `update-prerequisites` (`--report-only`, fixed), `require`, `report-compliance`, `bootstrap --dry-run` — with the session tools. When the last compile found prerequisites undeclared, a card above it carries each finding and "Write them into the estate", which is `WritePrerequisites`: satz's own writer under the write lock, checked and reloaded like an answer |
| Deploy | `src/views/deploy.rs` | what hands the estate off: the `hcl_dir` path with two chips saying whether `main.tf` is written and whether the directory is initialised, then the `DEPLOY` deck — `transpile`, `hcl-init`, `plan` in the app; `apply`, `migrate`, `bootstrap` as command lines to copy or open in the terminal |
| Chat | `src/views/chat/` | the agent loop over the open estate: the rail of this estate's transcripts with "New" (empty on the Claude Code engine, which keeps its conversation in its own process), the turns as they stream with one card per tool call and the approval card, the composer with the model, the effort and the capability chips, and the usage footer. `mod.rs` holds the status card for the states the engine is not in — `Starting` a progress line, `NoCredential` and `NotSignedIn` the two empty states below, `Failed` the error with Open Settings |
| Settings | `src/views/settings.rs` | every `Settings` field as a form: the satz path with the detected version, the MCP ceiling, auto-approve, the provider with base URL and model for the non-Claude ones, the Claude model, effort, fallbacks, transcripts, theme; Save writes the file and locates satz again; the credential card shows where the Claude credential comes from, whether that engine is the one in use, and stores a key in the keychain; the Claude Code card shows the binary, the account and which engine is in use, with "Use this engine", "Sign in" and "Sign out", and the stream log switch with "Reveal logs"; beside the satz path, "Update satz" and "Check only" run `satz self-update` (with `--no-open-readme`, so a successful update does not open a browser) and stream it into a log card, and satz is located again once it installs. While satz is newer than the build, the line under the path names both versions and whether satz calls the difference a patch or a minor. Under the buttons `SatzReleaseActions` (`src/views/satz_release.rs`): what the satz check found — a newer satz, the latest, or why the look failed — or why none ran at launch; "Look for a satz-studio update" with its sentence below it (or why it failed) and "Open satz-studio X" when a newer release exists; while no satz is found, "Install satz" with Cancel and the install's log card — on Windows, and while a satz path is set, the sentence saying why the installer is not offered |
| Commands | `src/views/commands.rs` | not a destination: `PALETTE` is the table of every satz command the app runs, and `CommandDeck` renders any group of them — the list, the chosen entry's argument fields (a reporting command's format as a segmented button), the command line as it will run, Run and Cancel, and `CommandLog`, the streamed log with stdout and stderr distinguished, followed by the file a reporting command wrote where the app named it. `CommandPalette` is every entry in a dialog over the window, on ⌘K / Ctrl+K or the top bar's button, with the session tools `satz_whoami`, `satz_transpile_check`, `satz_questions` as one click each. `CHECKS` and `DEPLOY` are the two groups the destinations gather |
| Gallery | `src/views/gallery.rs` | every component in its variants, light and dark side by side. A development route: the rail offers it only with `SATZ_STUDIO_DEBUG` set, and nothing else navigates to it |

A destination that works on an estate shows a card with a button to the Start screen
while none is open — which the rail cannot reach with no estate, but the banner and the
chat's empty states can.

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
| `Tabs`, `Tab` | `.m-tabs`, `.m-tab` | <https://m3.material.io/components/tabs/specs> | primary tabs only; fixed tabs, no scrollable row, no swipe |
| `Dialog` | `.m-dialog` | <https://m3.material.io/components/dialogs/specs> | basic dialog only; no full-screen dialog; a `class` prop widens one whose body needs it (the commands palette) |
| `List`, `ListItem` | `.m-list`, `.m-list-item` | <https://m3.material.io/components/lists/specs> | one- and two-line items; no three-line item, no dividers |
| `Tree`, `TreeItem` | `.m-tree` | — (not a Material 3 component) | a nested list with a disclosure per branch, styled with list-item tokens; a `trailing` slot at the row's end |
| `ConnectorTree`, `ConnectorBranch` (`ConnectorLine::Solid`, `Dashed`; `error`, `trunk_error`, `label`) | `.m-connector-tree` | — (not a Material 3 component; the anatomy is the indented tree of a file explorer, with boxes for rows) | boxes joined by right-angle connectors: a root box, then per branch a vertical trunk from the parent and a horizontal branch into the child box, recursing through `branches`. The connectors are 2 px borders on the tree's own elements in `outline`, the error variant in `error` — nested lists and absolutely placed spans, no SVG and no measuring, so a tree cannot cross itself and follows every resize and zoom. The branch meets its box 32 px below the box's top, the centre of a card's head row under its padding; a head that wraps meets the connector above its centre. A `label` is one line of 20 px above the box, cut with an ellipsis. No disclosure: every branch is shown |
| `ChipList` | `.chip-list` (in `views.css`) | — (input chips over a text field) | the values of a list as removable input chips, a field that adds one on Enter or blur, several with commas |
| `TypedField` | — | — (composes `Switch`, `TextField`, `ChipList`) | one field in the shape satz reads a value in (`FieldKind`: bool, number, list, text) over a `Draft`; a brace in a text and a non-number in a number field are refused under the field with satz's sentence, and `oncommit` fires only for a draft without a problem |
| door card | `.door` (in `views.css`) | <https://m3.material.io/components/cards/specs> | a `Card` with `onclick` carrying an icon, a title and a supporting line; the chosen door is the filled variant on the primary container. One per `Door`, in the Start screen |
| `SourceChips` | `.source-chips` (in `views.css`) | — (assist chips over literal text) | a value as satz reads it: a `{param}` chip with what it resolves to, a `${…}` reference chip, the literal text between |
| `Badge` | `.m-badge` | <https://m3.material.io/components/badges/specs> | — |
| `LinearProgress`, `CircularProgress` | `.m-linear-progress`, `.m-circular-progress` | <https://m3.material.io/components/progress-indicators/specs> | the linear indicator has the Expressive stop indicator; the wavy Expressive variant is not drawn |
| `Tooltip` | `.m-tooltip` | <https://m3.material.io/components/tooltips/specs> | plain tooltip only, below the anchor, no rich tooltip |
| `NavRail`, `NavRailItem` | `.m-nav-rail` | <https://m3.material.io/components/navigation-rail/specs> | collapsed rail only (88 px); no expanded rail, no menu button |
| `TopAppBar` | `.m-top-app-bar` | <https://m3.material.io/components/top-app-bar/specs> | small top app bar only; no scroll behaviour |
| `Snackbar` | `.m-snackbar` | <https://m3.material.io/components/snackbar/specs> | an error toast uses the error-container colours, which the spec does not define for snackbars |
| banner (`SatzBanner`) | `.banner` | — (not a Material 3 component) | an error-container surface with the text and its buttons: "Update satz" when a binary was found and refused as too old, "Install satz" (and Cancel while it runs) when none was found, then "Try again" and "Settings". For a satz newer than the build, `.banner--notice` on the tertiary container — a notice, not a fault — with the launch look's sentence under the text, "Open satz-studio X" when it found one, "Look for a satz-studio update" when it has not or failed, and "Dismiss" |

No Material Web Components and no other library are used: the components are Dioxus
components over these classes. The Gallery view is the checklist: every Material
component above in every variant, and the tree and the connector tree (a dashed and an
error branch among its three levels), in the light and the dark scheme side by side; the
three composites (`ChipList`, `TypedField`, `SourceChips`) are seen in the estate views
and the door card on the Start screen, and their classes live in `views.css`, so
`components.css` stays the component anatomies alone.

### Shell

- **Navigation rail:** six primary destinations in the order the work happens —
  Overview `dashboard`, Packs `inventory_2`, Decisions `quiz`, Estate `description`,
  Checks `fact_check`, Deploy `rocket_launch` — and a bottom-aligned group of two, Chat
  `chat` and Settings `settings`, in the rail's footer slot. Overview carries the count
  of what the estate owes and Decisions the count of unanswered questions. With no
  estate open the primary group is empty and the footer carries Settings alone: there is
  nothing to work on, and the window stands on the Start screen. `SATZ_STUDIO_DEBUG`
  adds Gallery `palette` to the footer. There is no FAB: opening an estate is what the
  Start screen does, and switching one is the top bar's action.

  **The pattern's limit, so it is not argued later.** Material 3 puts three to seven
  destinations in a navigation rail
  (<https://m3.material.io/components/navigation-rail/guidelines>). Six primary plus a
  group of two is inside it because Chat and Settings are bottom-aligned SECONDARY
  items, not peers of the six. A seventh PRIMARY destination breaks the pattern, and
  the answer then is a navigation drawer — not a smaller font, not a denser rail, not an
  eighth icon. `crates/satz-studio/src/state/mod.rs` holds `View::PRIMARY` and a test
  that fails outside three to seven.
- **Top bar:** the estate's file name and directory; the `runs_as` chip ("runs as the
  ADC identity" when the estate impersonates nothing); the satz-studio version chip,
  "satz-studio X · update available: Y" when the launch look found a newer release, which
  opens that release's page; the satz version chip, red while satz is missing, too old or
  does not run, and "satz X · update available: Y" when the satz check found a newer satz,
  which runs `satz self-update` ("updating to Y…" while it runs); the commands palette, reload and "Switch estate", which
  closes the estate and returns the window to the Start screen; the drawer toggle with
  the diagnostics count. The bar carries what is true of the WINDOW — which estate is
  open, whom it acts as, which satz compiles it. What is true of the ESTATE — its
  deployment mode, its schema, how far its HCL has been taken — is the Overview's
  identity card, so each fact has one place.
- **Commands palette:** every entry of `PALETTE` in a dialog over the window, opened
  with ⌘K (Ctrl+K on Windows and Linux) or the top bar's button, and closed with
  Escape, the scrim or the same key. The listener is installed on the window in
  `src/app.rs`, which mounts once: a keydown inside a text field never reaches a handler
  above it, and a listener per estate would leave one behind on every switch.
- **Banner:** while satz is missing, too old or does not run, a full-width error banner
  on every view naming the fix, with "Try again" and "Settings": for a too-old satz
  "Update satz" (`satz self-update`); for none at all "Install satz" — satz's own
  installer, checked against the SHA-256 its release publishes, writing
  `~/.local/bin/satz` and leaving the shell profile alone — unless Settings name a satz
  path, where the banner says to correct or clear it; on Windows the banner says satz
  publishes no Windows build, so it cannot be installed there, and offers no install.
  While satz is newer than the build, a notice banner instead, and every estate opens:
  "satz X is installed; this satz-studio was built and tested against satz Y", then satz's
  own reading of the difference — a patch: "satz says nothing an estate needs changes"; a
  minor: "satz says an estate may need edits, be refused, or plan differently". Under it,
  what the launch look found for satz-studio: a newer release (with "Open satz-studio X",
  its release page in the browser), that this is the latest release, or why the look failed
  — GitHub not reachable, or the API's hourly limit of the unauthenticated address with when
  it resets — with "Look for a satz-studio update" to look again. "Dismiss" hides the notice
  for that satz version; the next newer satz shows it again. The look downloads nothing and
  installs nothing.
- **Window title:** `satz-studio <version>`, then "— update available: satz-studio Y" and
  "— update available: satz Y" for what the launch looks found. The title bar is not
  clickable; the top bar's chips are where either is acted on.
- **Diagnostics drawer:** a bottom drawer, collapsed to its header, listing the open
  estate's diagnostics grouped Errors, Warnings, Notes, each with `file:line` relative
  to the estate directory, its source, and — for one of satz's findings — a chip naming
  the check that raised it (`unadopted-pack`, `missing-required`, …); clicking a row
  sets the selection.
- **Snackbar host:** the toasts, three at most, a notice for five seconds and an error
  for twelve, each dismissable.

### The two engines

`Settings.provider` decides which engine the chat runs on: the Messages API, or the
installed Claude Code CLI on the user's claude.ai subscription
([ADR 0010](adr/0010-claude-code-as-the-subscription-backend.md)). Being signed in to
Claude Code and running on Claude Code are two different statements, so every surface
that shows one shows the other beside it. `EngineOffer` in `src/views/settings.rs` is
that decision — `Checking`, `InUse`, `Ready`, `SignedOut`, `Absent` — and both views
render from it.

- **The Claude Code card (Settings).** The binary path with the located version as its
  supporting text, then the state line: the account as `claude auth status` reported it
  (`signed in as first.admin@example.com via claude.ai`) followed by `and in use` or
  `not the selected engine`; `not signed in` and `no CLI to ask` in the error colour.
  "Use this engine" appears only in the `Ready` state, sets the draft's provider to
  Claude Code and saves through `AppAction::SaveSettings` — the same action the Save
  button sends, so it saves the whole draft. The provider segmented control above stays
  the primary selector; this button is a second door to it, not a second setting. "Sign
  in" and "Sign out" write a one-shot script and open the user's terminal, because the
  login opens a browser. Below them, the switch "Log every line Claude Code and the app
  exchange" edits `claude_code_log` in the draft, saved with the rest; under it one
  sentence says what a log holds — "The log holds the estate's contents, its resource
  names and everything you type, and never leaves this machine." — and a state line
  (`.settings__status`, a `folder` icon) gives the bounds from `log::MAX_FILES` and
  `log::MAX_BYTES` (one file per conversation, the ten newest kept, each up to 16 MiB,
  applied from the next conversation) beside a text button "Reveal logs", which creates `<data dir>/satz-studio/logs/claude-code/` when it
  is not there and opens it with `open::that`, the opener "Show file" uses; a failure is
  an error toast naming the directory. The credential card carries the same second half
  of the sentence: `in use` when the provider is Claude, `not the selected engine`
  otherwise.
- **The chat's empty states.** `NoCredential` (the Messages API resolved no credential)
  probes Claude Code once while the card renders — `ClaudeCodeCli::locate` then
  `auth_status`, spawned from a `use_hook` so it runs once per mount and never on the
  render. Signed in, the card leads with a primary-container block: "Claude Code is
  ready", the account, and a filled "Use Claude Code" that selects that provider, saves
  the settings and rebuilds the engine, with the four API sources and what each answered
  below under "Or use the Messages API". Not signed in or not installed, the card is the
  four sources as before plus one line naming Claude Code, the CLI's own reason when it
  did not answer, and `claude auth login`. `NotSignedIn` (Claude Code selected, CLI
  signed out) is its own card with "Sign in" and "Check again".
- **The composer.** On Claude Code one assist chip, "tools run inside Claude Code": the
  loop, the context window and the effort are Claude Code's, so the effort control is
  hidden and the model field is disabled with "Claude Code takes its model from
  Settings". On the API engine the chips name what the provider lacks — no thinking, no
  effort, no caching — and a provider without tools gets a line above the input.
- **The footer.** The turn's and the session's tokens, the model, and where the
  transcript is kept ("not kept" on Claude Code). A `rate_limit_event` from the
  subscription arrives as `AgentEvent::Notice` and shows there beside a
  `data_thresholding` icon: how much of the five-hour or seven-day plan window is used
  and when it resets.

Deviations from the Material 3 specification these introduce:

| surface | class | spec page | deviation |
|---|---|---|---|
| the empty state's lead | `.chat__lead` (in `chat.css`) | — (not a Material 3 component) | a primary-container block inside a filled card, to put the engine that is ready above the alternative; the spec has no nested-surface anatomy for this |
| the state line | `.settings__status` (in `views.css`) | — (not a Material 3 component) | an icon, a sentence and a text button on one line inside a card; the Claude Code card has two, the account and the log's bounds |
| the plan-utilization notice | `.chat__footer` (in `chat.css`) | — (not a Material 3 component) | a footer line, not a Material progress or badge; the plan windows arrive as text and are shown as text |

## The smoke walk

The manual check of the estate views, over `tests/fixtures/smoke` (satz's own smoke
estate, read from the pinned submodule — copy the estate to a scratch directory before
a step that writes, as the fixture's `config.toml` says) and over a skeleton written by
`satz interview <dir>/yaml/new.satz --create`, which is the estate every pack line
starts commented in. Every write is checked by `satz transpile --check` through the
estate's `satz mcp` child. No step needs an API key; the first runs `satz init`, which
reads the Application Default Credentials where there are any; steps 12 and 13 need the
Claude Code CLI installed and signed in, and nothing else, and neither sends a message. Nothing in the walk changes a live
organisation: `bootstrap` and `apply` are read as command lines, never run.

0. **Create.** Estates → Create → a folder and a customer id → Create. The window lands
   on **Decisions**, not on Overview: the skeleton has 16 questions of its own and nothing
   else yet. Overview's identity card carries those 16 as rows, every one of them reading
   "not answered", and the chip in its header counts them.
1. **Import.** Import → an empty folder → Terraform HCL → a directory
   holding a `.tf` file. The card under the folder says satz init runs first; the
   preview shows both command lines. Import → the log carries both runs, the report
   card names the file satz wrote and every block it promoted or wrapped, and that
   estate is open in the top bar. Choose "State document" and point it at a raw
   `.tfstate`: the field turns red with satz's own sentence and Import stays disabled.
2. **Open.** Open → the folder → Open. The window lands on Overview; the top bar shows
   the file and "runs as the ADC identity". The first card is the identity card: the
   customer this estate stands for, by name, short name, customer id, organisation id and
   domain, then the infrastructure it names and the service account it runs as, each one
   the estate's own answer and an unanswered one saying so; below them the deployment
   mode, the schema with its provider and type count, and what the HCL directory holds.
   The rail carries the owed count on Overview and the unanswered count on Decisions.
3. **Read what is left.** The Overview's "Still to do" card lists a row per thing and
   nothing else. An estate made in a folder outside any repository carries "The estate
   is not in a git repository" first; "Create the repository" streams `git init -b
   main`, `git add -A` and the commit into the log and the row goes (with no git
   identity configured, the log ends in git's own refusal and the row stays). Answer a question (step 4) and the questions row loses one; answer the
   last and the row goes. Point the estate at an empty `schema_dir` (step 9) and the
   schema row appears with "Run update-schema" on it. An estate that owes nothing shows
   "Nothing left to do" and no card of rows.
4. **Answer a question.** Decisions → the first open question → Accept (or type a
   value and Answer). A list question that offers nothing — the access-approval pack's
   `access_approval_notification_emails` on an estate that uses it — is a chip list
   with "no default — at least one value is needed"; one address and Enter writes
   `access_approval_notification_emails = ["…"]`, a list of one. The toast says "1 answer written"; the file has one new line in
   `params { }` (`git diff` shows nothing else: no re-emission, comments and alignment
   intact); the question count on the rail drops by one; a diagnostic the compile had
   raised for that param is gone from the drawer.
5. **Enable the map, then a pack.** On the skeleton: Packs → "Enable the map" → the line
   `use "presets/estate-map.satz"` is uncommented and the map's questions are open.
   Toggle `use_budget` on → the answer lands as `use_budget = true` and satz
   uncomments `use "presets/organization-budget.satz" when use_budget`; toggle it off
   → `use_budget = false` and the line stays active — the card says so, and the drawer
   carries the model's note "line active, gate false" on that line, shown inline on the
   card. The Security Command Center card is a block across the grid with notifications
   and export hung below it and the mail and SIEM cards below notifications, each with
   the phase it left as a caption; the Sentinel card has its two log paths below it,
   dashed and captioned "follows `use_sentinel`". Answer `use_scc_enablement` yes, then
   `use_scc_notifications` yes, then `use_scc_enablement` no: the connector into
   notifications turns the error colour and reads "on while `use_scc_enablement` is off".
   Narrow the window to its minimum and widen it again: the connectors stay joined to
   their cards.
6. **Edit an attribute.** Estate → Resources → a resource → a string row → change it → Enter.
   The toast names the file; the line shows the new value with its `=` column where it
   was; the rest of the file is byte-identical.
7. **Type a brace.** Estate → Params → a text row in value mode → type `{x}` → the field turns
   red with "braces interpolate in a Satz string — if `{x}` is what you mean, write
   that param by hand" and nothing is sent; the source-mode toggle is where an
   interpolation is written.
8. **Break a value.** Estate → Params → source mode on a row → replace the value with a
   bare name nothing binds → Enter. The toast says "not written — line N: …"; the drawer
   shows the check's diagnostic at that line, source "check", and it stays after the
   reload; clicking it in the drawer opens Estate → Resources with the node at that line
   selected; the file is byte-identical to before.
9. **Missing schema.** Point a copy's `schema_dir` at an empty directory and open it:
   the Overview carries the schema row and its "Run update-schema", the identity card
   says "none in <dir>", and Estate → Resources locks every row with "no schema".
10. **The palette and the two halves of day 0.** ⌘K opens the commands palette over
    whatever destination is showing; Escape closes it. Checks → `update-prerequisites`
    runs `--report-only` and prints the gap; where the compile found one, the card above
    the deck offers "Write them into the estate" and the toast counts the lines written.
    Deploy → `apply`, `migrate` and `bootstrap` offer "Copy" and "Open in terminal" and
    no Run; `bootstrap --dry-run` lives in Checks and runs here. **Do not run a real
    `bootstrap`** against an organisation you are not prepared to change.
11. **Switch estates.** The top bar's "Switch estate" closes the estate and returns the
    window to the Start screen with its three doors; the rail carries Settings alone.
12. **Switch engines from the chat.** With `provider = "claude"` in `settings.toml`, no
    `ANTHROPIC_API_KEY` in the environment and the Claude Code CLI signed in: Chat →
    the card leads "Claude Code is ready" with the account, the four API sources below
    under "Or use the Messages API" → "Use Claude Code" → the toast says "Settings
    saved", the card goes, the composer shows "tools run inside Claude Code" and a
    disabled model field, and `settings.toml` reads `kind = "claude_code"`. Settings →
    the Claude Code card says "and in use" and offers no "Use this engine"; the
    credential card says "not the selected engine". Sign the CLI out
    (`claude auth logout`), set the provider back to Claude, reopen Chat: the card is
    "No Claude credential" with one line naming `claude auth login`.
13. **The stream log.** With the Claude Code CLI signed in and Claude Code the selected
    engine: Settings → the Claude Code card → switch "Log every line Claude Code and the
    app exchange" on → Save → Chat → "New" → Settings → "Reveal logs" → the file manager
    opens `logs/claude-code/` under the app's data directory, holding one `.log` named
    after the instant the session started. Its first line is a `studio` record naming
    the estate and the command line, then a `stdin` record with the initialize request
    and a `stdout` record with its answer, each line exactly as it went over the pipe.
    Switch the log off → Save → "New": no new file appears.
14. **A satz newer than the build, and the release looks.** At launch the window title
    reads `satz-studio <version>`, and Settings → the satz card says what the satz check
    and the satz-studio look found (with `self_update_frequency = "never"` in
    `~/.config/satz/satz.toml`, the card says the satz check did not run and why). Write a
    script that answers `--version` with a version one patch past the build's satz and
    hands everything else to the installed satz — `#!/bin/sh`, then `if [ "$1" =
    --version ]; then echo 'satz 0.59.8'; else exec ~/.local/bin/satz "$@"; fi` for a build
    against 0.59.7 — make it executable, and set it as the satz binary in Settings → Save.
    The tertiary banner names 0.59.8 and the build's satz and says it is a patch release,
    with the satz-studio look's sentence under it; the top bar's satz chip is not red; the
    Open door opens the fixture estate. "Dismiss" → the banner goes and `settings.toml`
    reads `dismissed_satz = "0.59.8"`. A second script like it answering `satz 0.60.0`, set
    as the satz binary → Save: the notice is back and says it is a minor release. Clear the
    path → Save, and the installed satz is in use again.
