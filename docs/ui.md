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
         ├─ NavigationRail   six primary destinations, Agent and Settings at the foot
         ├─ TopBar           estate name and directory, reload, switch, close
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
| `drawer_open` | the diagnostics drawer |
| `create` | the `CreateStore`: the one `satz init` run behind the Create door — `log`, `running`, `command`, `outcome`, reset when a run starts. What `init` derives from the credentials is in these lines while the window shows them and in the estate satz wrote; it reaches no file of the app's own |
| `estate` | the `EstateStore`: `model`, `cst` (the main file's document tree as read at the last reload — the views slice a value's source text and a line's text from it), `questions`, `interview` (what the last `satz_interview` call returned; `rename_to` is read from it), `diagnostics`, `command_log`, `running`, `last_command`, `outcome`, `loading`, `hcl` (the `HclState` of `hcl_dir`: whether `main.tf` is there and whether it has been initialised), `work_tree` (whether git holds the estate file's directory in a work tree — `None` until the first reload has asked), `export_formats` (the formats `satz questions --help` lists, read when the session opens, or why they could not be read), `last_export` (the format and the file of the last export, which "Export again" writes over), `review` (the pack the Packs view reviewed last — the bytes it judged and satz's report — or the pack and why its review failed; a reload leaves it, and the drawer shows its findings until it is closed), `reviewing` (a review or a placement is running) — reset when an estate opens or closes |

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
  and `SaveSettings` (the file, then satz again).
  At startup, after locating satz, it looks for releases once ([ADR
  0014](adr/0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md)): the latest
  satz-studio release into `studio_look`, and `satz self-update --check-only` on the satz
  in use into `update.found` — unless the operator's `~/.config/satz/satz.toml` says
  `self_update_frequency = "never"`, which puts the reason in `update.not_checked`. A look
  started at launch raises no toast when it fails; a run the operator asks for does.
  `UpdateSatz` refuses a second run while one is running. On Windows, where
  `satz self-update` checks but does not install, `UpdateSatz` without `check_only` runs
  satz's PowerShell installer instead (`install::update_by_installer`), verified as for
  an install and run with `SATZ_INSTALL_DIR` set to the folder of the satz in use, and
  calls the update done only if the satz then located is the release it installed. `DismissSatzNotice(version)`
  writes the version into `Settings.dismissed_satz`, which hides the banner's notice for
  that satz version and changes nothing else. `LookForStudioUpdate` reads the latest
  satz-studio release again from a task of its own, one look at a time. `InstallSatz` runs satz's installer
  (`state/install.rs` over `satz::install`) — `satz-installer.sh` under `sh`, or
  `satz-installer.ps1` under `powershell -File` on Windows — while the status is `Missing`
  and Settings name no satz path — the installer writes `~/.local/bin/satz`
  (`satz.exe` on Windows), which the search does not look at while a path is set —
  streams it into `install.log`, locates satz again, and calls the install done only if
  satz is then found; `CancelInstall` stops it.
  `OpenEstate` puts the window on `View::Overview` when the session opens and
  `CloseEstate` puts it back on `View::Start` — the window has nowhere to stand without
  an estate, so closing one IS switching estates.
  `CreateEstate` checks the target, runs `satz init` in it through `SatzCli::run_in`
  from a task so the loop stays free for `CancelCreate`, streams the lines into
  `create.log`, and then reads what the run left with `init::created`: one estate is
  opened, none is the outcome saying `init` had no customer id to name a file after, and
  a non-zero exit carries satz's own last line rather than a status number.
- **The estate coroutine** (`src/state/estate_actions.rs`, `EstateAction`) is started by
  `EstateHost` with the session and lives as long as the estate is open. On start and on
  `Reload` it reads `hcl_dir` into `estate.hcl`, asks git whether the estate file's
  directory is in a work tree (`WorkTree::read`) into `estate.work_tree`, calls
  `satz_questions` and `satz_packs` over the session,
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
  `CancelCommand`. A `RunCommand` or an `InitRepository` refused because a command is
  running leaves that command's cancel token in place, so `CancelCommand` still stops
  it. `OpenInTerminal(args)` writes the one-shot script
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
    then `Snapshot::delegate` around `satz_interview {answers: {subject: value}}` (for a
    `oneof` the value is the chosen option's param name), with `McpChecker` on the real
    path. A refused tool call is satz's own sentence in a toast and in the drawer (the
    brace refusal included); when satz had changed the file before it refused, or a call
    that returned no result had, the bytes are put back and the sentence ends "satz
    refused and had changed `<file>`; the file is back as it was". A check that refuses
    restores the bytes, puts its diagnostics in
    the drawer — they stay through the reload that follows — and a toast names the
    first line; a check that passes puts the findings it reported there instead, so a
    warning shows at its line beside the write that landed. `AcceptDefaults` is the
    same call with `accept_defaults: true`.
  - `CommitEdit(Edit)` is the app's writer: `EditSession::open`, `apply(&[edit])`,
    `Proposed::commit(&McpChecker)`. An edit the document layer refuses (`EditError`)
    is a toast and nothing was written; `Rollback::Check` is diagnostics and a toast as
    above; `Rollback::ChangedOnDisk` is the toast "changed on disk — reloaded". A commit
    that lands carries the check's own findings into the drawer, as an answer does.
  - `AddPack(AddPackArgs)` and `RemovePack(RemovePackArgs)` are satz's own pack logic
    under the same delegated-write discipline: `satz_add_pack` binds the pack's gate
    true (an option of a choice sets its siblings false) and uncomments its line, or
    writes it where the pack graph places it — the map's line as much as any other;
    `satz_remove_pack` binds the gate false and leaves the line. A switch satz refuses —
    a pack it needs is off, a pack that needs it is on, a line not gated on its gate —
    is satz's sentence in a toast and a diagnostic in the drawer, beside what the
    reload's check says of the file, and the file is compared with its record and put
    back as an answer's is. A switch that lands says what it switched and
    what it opened, and raises the notices it opened as an answer does. The app writes
    no pack line itself.
  - `WritePrerequisites` is the same delegated write over `satz_update_prerequisites
    {report_only: false}`: satz works out offline which roles the IaC service account
    lacks and which APIs the infra project does not enable, and writes both into the
    estate. `delegated_write` is the one implementation the two share.
  - `MergePresets` calls `satz_merge_presets`, which reconciles the estate's library
    with upstream. The log gets the report as sentences (`MergeReport::lines`): one per
    pack the merge changed or would change, satz's warnings, notes and prerequisites,
    the counts, and each notice it opened with the command that notice names. A report
    with `attention` — what makes `satz merge-presets` exit non-zero — is a failed
    outcome and a toast, as a command that exits non-zero is. A refusal is satz's own
    text; a report the app cannot type is a failed outcome and a toast.
  - `ReviewPack { pack, against }` runs `satz --config <estate dir> review-pack <pack>
    --format json`, with `--against <main file>` when `against` is set, on a task of its
    own so the loop stays free (`review::review`, [ADR
    0019](adr/0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md)).
    The result replaces the last review in `estate.review`; the toast is satz's verdict
    with the errors of a pack that does not clear the bar or the warnings of one that
    does (`review_verdict`), and the drawer opens when a finding is more than an info. A
    review that failed keeps the pack and satz's reason. A second review while one runs
    is refused with a toast.
  - `PlacePrivate` places the reviewed bytes in `presets_dir` as `<stem>.local.satz`
    under the write lock (`review::place_private`), the estate checked by
    `satz_transpile_check` with the file there. Placed, the toast names the file and
    the placed file is reviewed where it now stands; already there with these bytes,
    the toast says nothing was written; a file of that name holding other text, a pack
    edited since its review, or a missing library is refused with a toast; a check that
    refuses removes the file again, and its diagnostics join the drawer.
  - `CloseReview` drops the review, and its findings leave the drawer.

An error from either — a refusal, a missing binary, a function another unit has not
built — is a toast in the snackbar and, where it concerns the estate, a diagnostic in
the drawer or an outcome under the log.

### Views

The window is ordered by the job, not by what was built when. With an estate open the
rail reads **Overview, Packs, Decisions, Estate, Checks, Deploy**, then Agent and
Settings at its foot; with none open it carries Settings alone and the window stands on
the Start screen. Packs stands before Decisions because a pack is what DECLARES a
question: a pack switched on in Packs opens the questions it asks, so the questions
Decisions lists are what Packs has just let in. An estate created here
lands on Decisions rather than Overview — a skeleton has nothing else yet — and every
other door lands on Overview; the rail's order is the same for all of them.

**Decisions and Packs/Estate are not duplicates, and this is the rule that says so:**
Decisions is the WORKLIST — what the estate has not decided yet, one question at a time,
with what each costs to change later. Packs and Estate are the STATE — which packs the
file runs, which params it binds, which resources it declares. A param that answers a
question is editable in both, on purpose: the question is where the choice is explained,
the file is where it lives. It is the same rule as "one fact, one place" — a param that
is a pack's gate in satz's pack graph shows in Packs and not in Estate, because there it
is a pack switch and not a value.

| view | file | what it does |
|---|---|---|
| Start | `src/views/estates.rs` | the way in: a row of three door cards (`state::Door`) over the pane the chosen door opens. **Open** is the folder (typed, or picked with the OS dialog), one card per `config.toml` with its estates, each with its deployment mode and an Open button; the open estate is marked and can be closed. It is where the window stands with no estate open, and where closing one returns it |
| Create | `src/views/create.rs` | the **Create** door: the folder the new estate goes in, picked or typed, refused while it is not there or already holds a `config.toml`; the fields satz cannot derive (customer shortname, default region, the Terraform tool as a segmented button, the Google provider set as a switch, extra providers as a chip list); the customer id and the billing account as overrides, empty by default and each saying which live call answers it when it is left blank; the command line as it will run; Create and Cancel; and the run log with stdout and stderr distinguished, a line saying what satz derives is printed there and kept nowhere else, and the outcome chip — satz's own last line when the run refused |
| Import | `src/views/import.rs` | the **Import** door: the folder the import runs in, picked or typed, with a line saying which of the two things will happen — `satz import` alone, or `satz init` first because the folder holds no `config.toml`; the source as a segmented button over the three shapes (state document, live scope, Terraform HCL) with the source field and its picker below, each shape saying in its supporting line what it reads — the state one that it is `tofu show -json` output and not a raw `.tfstate`; then the flags of that shape alone (only/exclude/all, the collision rule, the customer shortname, the output file and `--verbose` for state and live; wrap-all for HCL); the Terraform tool and the provider-schema switch when the init half runs; the command line, or both of them, as they will run; Import and Cancel; and, beside the log, satz's report split into what it wrote, what it skipped, the params it could not derive and its warnings, with "Check it compiles" running `satz_transpile_check` on the estate that opened |
| Overview | `src/views/overview.rs` | the identity card first — which estate this is, by its own answers: `identity()` opens with Short name, Customer, Customer ID and Organisation ID (`customer_shortname`, `customer_longname`, `customer_id`, `customer_organization_id`) read from the model's `params` — the file's own `params { }` block, whether or not the estate uses the `estate_core` pack — each reading its value, "not set" where the file does not set it (never a pack's default), "empty" where it is `""`, and "not read yet" until the model is built; then the other rows the `estate_core` pack declares, out of the questions report, in a written order (Domain, then the infrastructure, the IaC identity, the engine and the region), a core subject the order does not name under a label made from its own name, an unanswered one saying so rather than showing the pack's default, and `deployment_mode` left to the State row below it — the four subjects above are never a question row, so each shows once; then the Runs as row, `runs_as_row()` over what `satz_open` reported, telling apart the cases `satz whoami` does — the IaC service account impersonated by the ADC identity in cloud mode, the ADC identity itself in local mode with the `satz migrate --mode cloud` that switches it, and the ADC identity in cloud mode because the estate declares no account; then the file, directory, state, schema and HCL rows, and a chip counting the rows without a value — not answered, not set or empty. Below it "Still to do": what the estate owes, derived on every render by `owed()` over `Facts` — the estate not in a git repository (so `satz merge-presets` refuses and no preset update can land; git's own words, and "Create the repository" running `InitRepository`) or git not installed (said, with nothing to press), day 0 unconfirmed, questions unanswered, the map off or absent, the pack graph's findings about the packs the estate uses (the first one's sentence, and Packs as the destination), the pack lines with no gate that no finding covers — a `use` line without its `when <gate>` deploys the pack whatever the estate answers — no provider schema, packs whose notice is open — counted from the compile's own `notice` findings, with "Show the notices" where this session opened them and nothing to press where it did not, the drawer carrying each with its command and its param — prerequisites the compile found undeclared, raw HCL without a `trust` reason, an HCL directory that has not been compiled or not been initialised — each row with what to do about it: a destination, a command run here, or a terminal hand-off. That card is replaced by "Nothing left to do" when the list is empty; below it the export card as the handover record (`ExportCard { moment: Moment::Handover }`, see Decisions), and, once something has run, the shared command log follows |
| Decisions | `src/views/interview.rs` | the questions report one question at a time, unanswered first with a "Show answered" switch: the pack's description when the pack changes, the prompt, the `why`, chips for reversal and blast, a warning banner on a one-way door, the recommendation when it differs from the offer, the field in the shape `answer_kind` reads off the report — the `shape` the pack declares the param with, else the offered value's, which is the order satz's `parse_answer` reads an answer in, so a param a pack declares `[]` offers nothing and is still a list — as a switch, a number field, a chip list or a text field that refuses a brace with satz's sentence; a chip list takes several values typed at once with commas. A question whose param is declared as a map, or with no shape and nothing offered, shows no field and says the value is written into the estate's params by hand, a `oneof` as filter chips with the chosen option's `why`, starting on the option the estate binds, else the pack's default. The card has one filled button, and it reads **Accept** (`check`) or **Next** (`arrow_forward`) (`primary` in `src/views/interview.rs`): on a question not answered it reads Accept whether or not the value was changed — accepting the offered default writes it — and is disabled while the field has a problem, while it is empty on a question that offers nothing, while no chip is chosen, and while a write is in flight; on an answered question, or one not asked, it reads Next while the field holds the written value, or the chips the bound option, and Accept as soon as the value differs — putting the value back returns it to Next. Accept writes the card's value (one `Answer` action) and moves to the next question; Next moves and writes nothing. Changing the field or picking a chip writes nothing: a keystroke, a switch flip and a chip added to or removed from a list only change what the card holds, and the field losing focus writes nothing either (`ON_BLUR` and `ON_CHANGE` in `src/views/interview.rs`, over `commit_on_blur` and `commit_on_change` of `TypedField`) — Params and Resources keep saving on blur, on a flip and on a chip change, where that saves a value into the file being edited. Enter in a text or number field is the keyboard form of the filled button: it does what the button reads, and nothing while the button is disabled. Beside the filled button, the text button **Skip** on a question not answered, which moves on without writing; an answered question has no Skip. A question with no field has no filled button: its text button reads Skip, or Next where the question is answered or not asked. **Back**, a text button, returns to the question last left, switching "Show answered" on when that question is answered. The card follows its question by subject: Accept moves the walk over the report as it stands before the write, so the reload that takes the answered question out of the unanswered block leaves the card on the question after it — not one further, not back at the top — and a question that leaves the walk hands its place to the one that took it. A reload that changes the value a card starts on starts that card afresh (`card_key`). Accepting the last open question empties the walk, and Back returns to it. Back, Skip and the filled button stand in an action bar below the question, outside its scroll: the pack header and the card scroll above it, the bar stays in view and at the same place on every question — Back at the leading end, Skip and the filled button at the trailing end, on the surface container (`.interview__actions`, `.interview__scroll` in `assets/css/views.css`). The bar takes the arrangement of a bottom app bar (<https://m3.material.io/components/app-bars/specs>) and departs from the component: it spans the question's column rather than the window, carries labelled buttons rather than icon buttons and a FAB, and has corners and no elevation; "Accept n defaults"; the list beside the card always, never only with the switch on — the switch decides what is IN it, unanswered alone or every question — each row its prompt, subject and answer, the one on the card selected, a click opening it, and, where the filter leaves nothing, the row that says so ("No unanswered question…"). The view fills the content area and does not scroll: the walk scrolls and the list scrolls, each to its own height, so a long list never moves the question — in a window under 1100px the two stack, the view scrolls as a whole again, and the action bar sticks to the bottom of the window while its question is in view. The progress bar carries the count and the completion in one line ("n of m answered · nothing open — bootstrap and apply will not refuse this estate for an open question"): there is no separate "every applicable question is answered" card and no second counter on the question card, which counted the filtered list and so disagreed with the bar. Then the `rename_to` card when the last `satz_interview` returned one. Below the walk the export card as the sign-off sheet (`src/views/export.rs`, `ExportCard { moment: Moment::SignOff }`): a segmented button of the formats the installed satz lists in `satz questions --help`, starting on `markdown` — the decisions sheet — when satz offers it, with satz's own line for the chosen one under it; "Export…" opens the OS save dialog in the estate's directory on `<estate>-decisions.<ext>` and sends `Export { format, out }`, which runs `satz questions <estate> --format <format> --out <file>` into the shared log and opens the file once it is there and not empty; after one, "Export again" writes the same format over the same file and the card names both. A help the app cannot read is the card's text and a toast, and Export is not offered |
| Packs | `src/views/map.rs` | satz's pack report (`EstateModel::packs`, from `satz_packs`) and nothing the app derives. The map first: a chip with its line and "Run merge-presets" beside it while it deploys, else a card saying its line is commented out (with the line) or absent, with "Switch the map on" (`AddPack`). When the presets carry no pack graph, satz's `note` is a card of its own. Then every pack whose line the file carries, in the file's order under the phase its line stands under (the phase comment's first line, from `EstateModel::phases`; a line without one joins the section open at that point), then every pack the file has no line for under "Not in this file", then the `use` lines the graph does not know under "Not in the pack graph", each a card with its path and line and no switch. A pack's card: its path and a chip with its line state — active, ungated, commented out, absent, forked or misplaced, with the line number, filled when the pack deploys and in the error colour for ungated and misplaced — the question that asks its gate, where the questions report has one, the gate as satz reports it (`use_budget = true (answered true)`, or the default), the fork the line names, a line gated on another param, and a switch bound to whether it deploys: on is `AddPack`, off is `RemovePack`. The core pack and a pack whose line is written by hand carry no switch and say why. Below, what it needs, each with whether it is met — every requirement that is not, and the met ones the tree does not already draw — with "Switch on" (`AddPack` of that pack) on one that one pack meets; what needs it; what it excludes; its notices with the command and the param that acknowledges it; and the compile's findings about it, each with the command satz names for it. A card whose line is ungated or misplaced and that carries no finding says what that state means and the edit that answers it (`line_note`): an ungated line names its gate, the `when <gate>` to write on it and the comment that takes the pack off; a misplaced one says the line stands outside the block the pack graph places the pack in. satz reports `ungated-pack` only while the file declaring the gate deploys, so with the map off that sentence is the only word about a line no answer switches off — and where satz does speak, its finding stands there instead. A finding about no pack the report has a row for stands above the sections. The requirements make a tree of the same cards (`sections(report, phases)`, a pure function): a pack hangs below the pack that alone meets its first requirement (`parent_of` — a requirement several packs can meet, the billing permissions' choice of a security-group model, hangs a card nowhere, and neither the map nor the core pack is a parent); a card nothing hangs from and that hangs from nothing stays a cell of its section's grid; a card others hang from is a block across the whole grid row — a `ConnectorTree` with the card at the top and the packs that need it below, in section order, recursing (the audit-log archive → the central alerts → the findings mail is three levels). A child hangs under its parent wherever its own line stands, and leaves its phase: a section left with nothing is not shown, and a child whose phase is not its parent's carries that phase's first line as a caption above its card. A connector is solid for a gate or a `requires` requirement and dashed for a data one — the child reads params the parent declares — whose caption names them ("reads `…`"); where the child deploys while its requirement is not met, the connector and the trunk leading to it are in the error colour and the caption reads "deploys while `<pack>` is off". Above the sections — and below the card saying the model is not available, when it is not — stands the pack review (`src/views/review.rs`, `PackReviewCard`): "Choose a pack…" opens the OS file dialog on `.satz` files, in the folder of the last pack reviewed, else the estate's `presets_dir`, and runs `ReviewPack`; the switch "Judge it inside <estate>" adds `--against` the open estate, for a pack that estate uses. A review shows the pack's path, satz's verdict as a chip ("clears the bar", or "does not clear the bar yet" in the error colour), the counts of errors, warnings and notes, "Review again" and "Close", the estate it was folded into and what the pack emits; then the findings as a list — each with its line or "the whole pack" and the command satz names for it, clicking one sets the drawer's selection — beside the pack's text with line numbers, the selected finding's line marked and scrolled into view. Below, the two destinations as cards. **Upstream** says the hand-over is manual until satz ships contribute-pack and lists what the pull request to the satz repository takes: the file as `presets/<stem>.satz` or in the folder of its kind, a clean review (checked, or in the error colour while an error stands), a changelog row, and no organisation in the file — the review reads a private shape as an error, and each one becomes a param; "Copy the path" and "Open the folder". **Private** says the pack stays in this estate's library under the `.local.satz` name updates never touch, that a missing prerequisite row still goes upstream, names the file it goes to and offers "Place in the library" (`PlacePrivate`); a pack that is already the library's own file says so instead, and a name that cannot be a pack (`.diff.satz`) says why. A failed review is an error-container row with the pack, satz's reason, "Review again" and "Close" |
| Estate | `src/views/estate.rs` | the main file in two tabs, `ParamsPane` and `ResourcesPane`, with the counts on each. A diagnostic chosen in the drawer opens the Resources tab, where its line is |
| Estate · Params | `src/views/params.rs` | one row per `ParamRow`, grouped by the asking question's pack (else "estate"): a typed field by `ParamKind` in value mode, or the Satz source in source mode — a row whose value carries a `{param}` or `${…}` opens there, with its parts as chips; the question's `why` as a tooltip, a one-way-door chip, a raw-line toggle showing the line; a commit on Enter, blur, a switch flip or a chip change is `CommitEdit(Edit::ReplaceParam)` with a `TypedValue` in value mode and `TypedValue::Raw` in source mode |
| Estate · Resources | `src/views/resources.rs` | two panes: the tree of `ResourceNode`s (an icon per kind, a resource's name, `use` lines as leaves, branches collapsed below depth 2, a chip with the count of required attributes not written) and the selected node's card: kind, type, line, the missing required names, then one row per `AttrRow` — a typed field by `AttrType` (string, number, bool, a list of one of them; everything else and `Unknown` in source mode), locked rows dimmed with the reason (`import-id`, computed, not in the schema), source mode with its chips; a commit is `CommitEdit(Edit::ReplaceValue)`. Without a schema every row is locked and the header carries "Run update-schema". A row clicked in the drawer selects the node at its line |
| Checks | `src/views/checks.rs` | what judges the estate: the `CHECKS` deck — `transpile --check`, `update-prerequisites` (`--report-only`, fixed), `require`, `report-compliance`, `bootstrap --dry-run` — with the one-click commands. When the last compile found prerequisites undeclared, a card above it carries each finding and "Write them into the estate", which is `WritePrerequisites`: satz's own writer under the write lock, checked and reloaded like an answer |
| Deploy | `src/views/deploy.rs` | what hands the estate off: the `hcl_dir` path with two chips saying whether `main.tf` is written and whether the directory is initialised, then the `DEPLOY` deck — `transpile`, `hcl-init`, `plan` in the app; `apply`, `migrate`, `bootstrap` as command lines to copy or open in the terminal |
| Agent | `src/views/agent.rs` | configuring an agentic client on the open estate and starting it: a lead card saying the app runs no model, with the capability ceiling as an assist chip, then one outlined card per client — **Claude Code** and **Claude Desktop**. Each card holds the block `satz mcp-config <estate> --client <client> --allow <ceiling>` printed, satz's notes under it in the secondary colour, and the row "Configure <client>" and "Copy"; the Claude Code card also carries "Open in <client>", which runs the configured command in the estate's directory in the OS terminal. "Configure <client>" runs the same command with `--write`: the line satz ends on is the toast and all of satz's words stand in the card. A refusal stands in an error container in that card, as satz wrote it, and only the one refusal satz answers with `--force` — its own key already there with other arguments — offers "Replace it". A command that is not configured disables "Open in <client>" and says Settings names the client; one that is not installed is an error toast naming it |
| Settings | `src/views/settings.rs` | every `Settings` field as a form, in three cards: **satz** — the path with the detected version, "Update satz", "Check only" and the run's log, `SatzReleaseActions`, and the MCP capability ceiling, which is the ceiling of the app's own `satz mcp` child and of the one an agent is given; **Agent** — the command line of the agentic client the Agent destination starts, with where that client is on `PATH` under the field, in the error colour when it is not installed or not named; **Appearance** — the theme; and below them the actions row with the file, "Show file", Discard and Save. Save writes the file and locates satz again. There is no credential and no engine here: the app runs no model |
| Commands | `src/views/commands.rs` | not a destination: `PALETTE` is the table of every satz command the app runs, and `CommandDeck` renders any group of them — the list, the chosen entry's argument fields (a reporting command's format as a segmented button), the command line as it will run, Run and Cancel, and `CommandLog`, the streamed log with stdout and stderr distinguished, followed by the file a reporting command wrote where the app named it. `CommandPalette` is every entry in a dialog over the window, on ⌘K / Ctrl+K or the top bar's button, with `ONE_CLICK` — `whoami`, `transpile --check` and `questions`, palette entries run at their defaults — as one click each. Every entry opens in the format a person reads: `text` where satz offers it, `markdown` where it does not (`report-compliance`); `json` stays a choice in the format picker, and no entry fixes a format in its fixed words — `update-prerequisites` runs `--report-only` in satz's text. The log shows what satz printed, and the file a reporting command wrote, as satz wrote it `CHECKS` and `DEPLOY` are the two groups the destinations gather |
| Gallery | `src/views/gallery.rs` | every component in its variants, light and dark side by side. A development route: the rail offers it only with `SATZ_STUDIO_DEBUG` set, and nothing else navigates to it |

A destination that works on an estate shows a card with a button to the Start screen
while none is open — which the rail cannot reach with no estate, but the banner can.

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
| `Dialog` | `.m-dialog` | <https://m3.material.io/components/dialogs/specs> | basic dialog only; no full-screen dialog; a `class` prop styles one whose body needs it (the commands palette, the notice dialog) |
| `List`, `ListItem` | `.m-list`, `.m-list-item` | <https://m3.material.io/components/lists/specs> | one- and two-line items; no three-line item, no dividers |
| `Tree`, `TreeItem` | `.m-tree` | — (not a Material 3 component) | a nested list with a disclosure per branch, styled with list-item tokens; a `trailing` slot at the row's end |
| `ConnectorTree`, `ConnectorBranch` (`ConnectorLine::Solid`, `Dashed`; `error`, `trunk_error`, `label`) | `.m-connector-tree` | — (not a Material 3 component; the anatomy is the indented tree of a file explorer, with boxes for rows) | boxes joined by right-angle connectors: a root box, then per branch a vertical trunk from the parent and a horizontal branch into the child box, recursing through `branches`. The connectors are 2 px borders on the tree's own elements in `outline`, the error variant in `error` — nested lists and absolutely placed spans, no SVG and no measuring, so a tree cannot cross itself and follows every resize and zoom. The branch meets its box 32 px below the box's top, the centre of a card's head row under its padding; a head that wraps meets the connector above its centre. A `label` is one line of 20 px above the box, cut with an ellipsis. No disclosure: every branch is shown |
| `ChipList` | `.chip-list` (in `views.css`) | — (input chips over a text field) | the values of a list as removable input chips, a field that adds one on Enter or blur, several with commas |
| `TypedField` | — | — (composes `Switch`, `TextField`, `ChipList`) | one field in the shape satz reads a value in (`FieldKind`: bool, number, list, text) over a `Draft`; a brace in a text and a non-number in a number field are refused under the field with satz's sentence, and `oncommit` fires only for a draft without a problem — on Enter, on a switch flip and a chip change unless `commit_on_change` is off, and on blur unless `commit_on_blur` is off |
| door card | `.door` (in `views.css`) | <https://m3.material.io/components/cards/specs> | a `Card` with `onclick` carrying an icon, a title and a supporting line; the chosen door is the filled variant on the primary container. One per `Door`, in the Start screen |
| export card (`ExportCard`) | `.export` (in `views.css`) | <https://m3.material.io/components/cards/specs> | an outlined `Card` holding a single-select `SegmentedButton` of satz's formats, a filled "Export…" and a tonal "Export again"; with five formats the segmented button is at the spec's maximum of five segments |
| pack review card (`PackReviewCard`) | `.review` (in `views.css`) | <https://m3.material.io/components/cards/specs>, <https://m3.material.io/components/lists/specs> | an outlined `Card` holding a `Switch`, a filled button, assist chips, a `List` of the findings beside the pack's text — a monospace block with line numbers, not a Material component — and two filled `Card`s for the destinations. The marked line uses the secondary container |
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
  Checks `fact_check`, Deploy `rocket_launch` — and a bottom-aligned group of two, Agent
  `smart_toy` and Settings `settings`, in the rail's footer slot. Overview carries the count
  of what the estate owes and Decisions the count of unanswered questions. With no
  estate open the primary group is empty and the footer carries Settings alone: there is
  nothing to work on, and the window stands on the Start screen. `SATZ_STUDIO_DEBUG`
  adds Gallery `palette` to the footer, and Commands stands in it between Agent and
  Settings. There is no FAB: opening an estate is what the Start screen does, and
  switching one is the top bar's action.

  **The pattern's limit, so it is not argued later.** Material 3 puts three to seven
  destinations in a navigation rail
  (<https://m3.material.io/components/navigation-rail/guidelines>). Six primary plus a
  group of two is inside it because Agent and Settings are bottom-aligned SECONDARY
  items, not peers of the six. A seventh PRIMARY destination breaks the pattern, and
  the answer then is a navigation drawer — not a smaller font, not a denser rail, not an
  eighth icon. `crates/satz-studio/src/state/mod.rs` holds `View::PRIMARY` and a test
  that fails outside three to seven.
- **Top bar:** the estate's file name and directory, and beside them — in the
  `TopAppBar`'s own `beside` slot, not at the bar's far end — what acts on that estate:
  reload, "Switch estate", "Close estate", and the reload spinner while a reload runs.
  Then, at the far end, the drawer toggle with the diagnostics count. Close returns the
  window to the Start screen as it stands; Switch returns it there with the Open door
  showing, so the estates it has found are in front of you.
  The bar carries the estate's ADDRESS and nothing else about it: what the estate IS —
  the customer it stands for, whom its commands run as, its deployment mode, its schema,
  how far its HCL has been taken — is the Overview's identity card, so each fact has one
  place. Neither version is here: the window title carries satz-studio's, a newer release
  of either is the banner's business, and Settings holds the satz binary with its update
  buttons, the release page and the look-again.
- **Commands palette:** every entry of `PALETTE` in a dialog over the window, opened
  with ⌘K (Ctrl+K on Windows and Linux) or the rail footer's Commands button — which sits
  between Agent and Settings, because the footer is where what is not a destination
  stands — and closed with
  Escape, the scrim or the same key. The listener is installed on the window in
  `src/app.rs`, which mounts once: a keydown inside a text field never reaches a handler
  above it, and a listener per estate would leave one behind on every switch.
- **Notice dialog:** what a pack asks to be run once it is switched on. A pack can name
  one command to run when it goes into an estate — `satz adopt` for the CIS org-policy
  packs, so every policy that is already live is in the state before the apply — and
  satz returns that notice in the report of the write that opened it (`satz_interview`,
  `satz_merge_presets`). The window holds them and raises a basic dialog over whatever
  destination the operator is on, one notice at a time, headed by the pack and carrying
  its sentence, the command as satz writes it, and what binding the notice's param
  means. Its actions are **Later** (lowers the dialog, the notice stands), **Run it**
  (the command in the app's log, over the Overview where that log stands beside the
  notice's own row, or in the OS terminal where the app runs that command)
  and **I ran it** (binds the param `true` through `satz_interview`, which is what an
  acknowledgement is). A command the app will not run — one that is not satz's, one that
  quotes a word, one carrying a placeholder other than `<estate>` — is shown with the
  reason in place of the button, and is the operator's to run. A notice leaves the
  window when the estate binds its param, whoever bound it: every reload asks satz
  (`EstateDir::acknowledged`) over the estate's own params, so `satz adopt --execute
  --import`, which binds it itself, takes the dialog away too.
- **Banner:** while satz is missing, too old or does not run, a full-width error banner
  on every view naming the fix, with "Try again" and "Settings": for a too-old satz
  "Update satz" (`satz self-update`, satz's PowerShell installer on Windows); for none at
  all "Install satz" — satz's own installer, checked against the SHA-256 its release
  publishes, writing satz into `~/.local/bin` and leaving the `PATH` and the shell profile
  alone — unless Settings name a satz path, where the banner says to correct or clear it.
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
  clickable; Settings is where either is acted on, and the banner offers the satz-studio
  release while it is naming a newer satz.
- **Diagnostics drawer:** a bottom drawer, collapsed to its header, listing the open
  estate's diagnostics and the findings of the pack the Packs view reviewed, grouped
  Errors, Warnings, Info, each with `file:line` relative to the estate directory, its
  source (`satz review-pack` for a review's), and — for one of satz's findings — a chip
  naming the check that raised it (`unadopted-pack`, `missing-required`, `pack`, …);
  clicking a row sets the selection and opens where its line is (`destination`): a line
  of the main file in Estate, a finding about the reviewed pack in Packs.
- **Snackbar host:** the toasts, three at most, a notice for five seconds and an error
  for twelve, each dismissable.

### The agent handoff

satz-studio runs no model
([ADR 0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
The Agent destination gives an external client the estate on screen, and Settings names
that client; there is no credential and no engine anywhere in the window.

- **What the cards show** is what satz printed. Each card runs `satz mcp-config
  <estate> --client <client> --allow <ceiling>` when the view opens and again when the
  estate or the ceiling changes, and renders its stdout as the block and its stderr as
  the notes under it. The binary and the root in that block are satz's own; the window
  states neither.
- **Configuring a client.** "Configure Claude Code" and "Configure Claude Desktop" run
  the same command with `--write`, which merges satz's key into the file that client
  reads and leaves every other server in it alone. The toast is the line satz ended on —
  written, added, replaced or unchanged — and the card holds all of it. A refusal stands
  in the card in the error container, word for word as satz wrote it; where satz's own
  refusal names `--force`, "Replace it" under it runs `--write --force`, and nothing
  else in the window passes that flag.
- **Copying.** "Copy" puts the block satz printed on the clipboard, for a client
  configured by hand.
- **Starting the client.** "Open in <client>" runs the configured command in the estate's
  directory, in the OS terminal, the way `apply` and `bootstrap` are run. The button is
  disabled and reads "No agent configured" while Settings name none; a command that is
  not installed is an error toast naming it. There is no terminal in the window, nothing
  is supervised, and the estate is re-read when the window comes back to the front.

Deviations from the Material 3 specification these introduce:

| surface | class | spec page | deviation |
|---|---|---|---|
| the lead | `.agent__lead` (in `views.css`) | <https://m3.material.io/components/cards/specs> | a filled `Card` laid out as one row — an icon, text, a chip — as `.deploy__state` is; the spec has no one-row card anatomy |
| the configuration block | `.agent__config`, `.agent__notes` (in `views.css`) | — (not a Material 3 component) | a monospace block on the highest surface container, selectable and scrolled at 260px, with satz's notes under it in the same face at the secondary colour; the spec has no code-block surface |
| the refusal | `.agent__refusal` (in `views.css`) | — (not a Material 3 component) | an error-container block inside a card carrying satz's words and the run that answers them, rather than a dialog: the question is about one file and the answer is in the card that raised it |

## The smoke walk

The manual check of the estate views, over `tests/fixtures/smoke` (satz's own smoke
estate, read from the pinned submodule — copy the estate to a scratch directory before
a step that writes, as the fixture's `config.toml` says) and over a skeleton written by
`satz interview <dir>/yaml/new.satz --create`, which is the estate every pack line
starts commented in. Every write is checked by `satz transpile --check` through the
estate's `satz mcp` child. No step needs a credential of any kind; the first runs
`satz init`, which reads the Application Default Credentials where there are any, and
step 12 needs an agentic client installed. Nothing in the walk changes a live
organisation: `bootstrap` and `apply` are read as command lines, never run.

0. **Create.** Estates → Create → a folder and a customer id → Create. The window lands
   on **Decisions**, not on Overview: the skeleton has 16 questions of its own and nothing
   else yet. Overview's identity card opens with the short name, the customer, the
   customer id and the organisation id as the skeleton's `params { }` block has them —
   the customer id given, a param `satz init` wrote as `""` reading "empty" — and carries
   the rest of those questions as rows below, each reading "not answered"; the chip in
   its header counts every row without a value.
1. **Import.** Import → an empty folder → Terraform HCL → a directory
   holding a `.tf` file. The card under the folder says satz init runs first; the
   preview shows both command lines. Import → the log carries both runs, the report
   card names the file satz wrote and every block it promoted or wrapped, and that
   estate is open in the top bar. Choose "State document" and point it at a raw
   `.tfstate`: the field turns red with satz's own sentence and Import stays disabled.
2. **Open.** Open → the folder → Open. The window lands on Overview; the top bar shows
   the file and its directory, with reload, switch and close beside them. The first card
   on the view is the identity card: whose estate it is, by short name, customer,
   customer id and organisation id, read from the file's `params { }` block, then the
   domain, the infrastructure it names and the service account it runs as, each one
   the estate's own answer and an unanswered one saying so; below them whom the live
   calls run as, the deployment mode, the schema with its provider and type count, and
   what the HCL directory holds.
   The rail carries the owed count on Overview and the unanswered count on Decisions.
3. **Read what is left.** The Overview's "Still to do" card lists a row per thing and
   nothing else. An estate made in a folder outside any repository carries "The estate
   is not in a git repository" first; "Create the repository" streams `git init -b
   main`, `git add -A` and the commit into the log and the row goes (with no git
   identity configured, the log ends in git's own refusal and the row stays). Answer a question (step 4) and the questions row loses one; answer the
   last and the row goes. Point the estate at an empty `schema_dir` (step 9) and the
   schema row appears with "Run update-schema" on it. An estate that owes nothing shows
   "Nothing left to do" and no card of rows.
4. **Answer a question.** Decisions → the first open question: the filled button reads
   Accept and Skip stands beside it. Type into the field: nothing is written, and the
   button still reads Accept. A list question that offers nothing — the access-approval
   pack's `access_approval_notification_emails` on an estate that uses it — is a chip
   list with "no default — at least one value is needed" and Accept disabled; one address
   and Enter adds the chip and writes nothing, then Accept writes
   `access_approval_notification_emails = ["…"]`, a list of one. Accept: the toast says "1 answer written"; the file has one new line in
   `params { }` (`git diff` shows nothing else: no re-emission, comments and alignment
   intact); the question count on the rail drops by one; a diagnostic the compile had
   raised for that param is gone from the drawer; the card is on the question that
   followed, and the answered one has left the list. Press Back: the switch goes on and
   the card returns to the question just answered, its chip reading "answered: …", its
   filled button reading Next and no Skip. Type into the field: the button reads Accept;
   restore the value and it reads Next again. Press Next: the card moves on and nothing
   is written. Shrink the window's height until the question does not fit: the question
   scrolls and the bar with Back, Skip and the filled button stays below it.
5. **Switch the map on, then a pack.** On the skeleton: Packs → "Switch the map on" →
   the line `use "presets/estate-map.satz"` is uncommented and the map's questions are
   open. Switch the budget pack on → `use_budget = true` lands and satz uncomments
   `use "presets/organization-budget.satz" when use_budget`; switch it off →
   `use_budget = false` and the line stays active, the chip reading "active" without
   the fill. Switch the billing permissions on → refused: the toast and the drawer carry
   satz's sentence naming the security-group models it needs, and the file is as it
   was. The audit-log archive card is a block across the grid with the central alerts
   and Sentinel hung below it and the findings mail below the central alerts, each with
   the phase it left as a caption; Sentinel's connector is dashed and names the params
   it reads. Switch the archive on, then the central alerts; switch the archive off →
   refused, naming the central alerts that need it, and the file is as it was. Narrow the window to its minimum and widen it again: the connectors stay joined to
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
11. **Leave the estate, two ways.** The top bar's "Switch estate" returns the window to
    the Start screen with the Open door showing and the estates it found listed; "Close
    estate" returns it there as it stands. Either way the rail carries Settings alone.
12. **Set an agent up on the estate.** Agent → the lead card says the app runs no model,
    with the ceiling as a chip, and each client card shows the block satz printed with its
    notes under it. "Configure Claude Code" → the toast is satz's line and `.mcp.json` is
    beside the estate's `config.toml`, its `mcpServers.satz.args` holding the estate's
    directory and that ceiling. Press it again → "unchanged" and the file's mtime does not
    move. Lower the ceiling in Settings → Save → Agent → "Configure Claude Code" → the card
    carries satz's refusal naming the key that is there, with "Replace it" under it;
    "Replace it" writes the new ceiling into the file. Put `{` in the file and press
    "Configure Claude Code" → satz's refusal says it is not valid JSON, no run is offered,
    and the file is untouched. "Copy" puts the block on the clipboard. "Open in claude" → a
    terminal opens in the estate's directory with Claude Code running, and `/mcp` there
    lists the satz server. Clear the agent command in Settings → Save → the button reads
    "No agent configured" and is disabled; set it to a name nothing installs → the field
    turns red under Settings and the button raises an error toast naming it.
    **Do not press "Configure Claude Desktop"** on a machine whose Claude Desktop
    configuration you are not prepared to have satz's key merged into.
13. **A satz newer than the build, and the release looks.** At launch the window title
    reads `satz-studio <version>`, and Settings → the satz card says what the satz check
    and the satz-studio look found (with `self_update_frequency = "never"` in
    `~/.config/satz/satz.toml`, the card says the satz check did not run and why). Write a
    script that answers `--version` with a version one patch past the build's satz and
    hands everything else to the installed satz — `#!/bin/sh`, then `if [ "$1" =
    --version ]; then echo 'satz 0.59.8'; else exec ~/.local/bin/satz "$@"; fi` for a build
    against 0.59.7 — make it executable, and set it as the satz binary in Settings → Save.
    The tertiary banner names 0.59.8 and the build's satz and says it is a patch release,
    with the satz-studio look's sentence under it; the
    Open door opens the fixture estate. "Dismiss" → the banner goes and `settings.toml`
    reads `dismissed_satz = "0.59.8"`. A second script like it answering `satz 0.60.0`, set
    as the satz binary → Save: the notice is back and says it is a minor release. Clear the
    path → Save, and the installed satz is in use again.
14. **Export the decisions.** Decisions → the sign-off card under the walk offers the
    formats `satz questions --help` lists (`text`, `markdown`, `pdf`, `json`, `xlsx` at
    the pinned satz), on `markdown` with nothing under it, on `xlsx` with satz's line "a
    workbook: the catalog a customer fills in and sends back". "Export…" → the save
    dialog opens in the estate's directory on `<estate>-decisions.md` → Save: the log
    shows the `satz questions … --format markdown --out …` line, the toast names the
    file, and it opens in the application the system gives Markdown. Pick `xlsx` →
    Export… → Save: the workbook opens in the spreadsheet application. Answer a
    question, then "Export again": the same file is written over and opens with the
    answer in it. The Overview's handover card offers the same, and "Export again"
    there writes the last export's file.
15. **Review a pack, then place it.** Copy
    `crates/satz-studio-core/tests/fixtures/review/team-access.satz` to a scratch folder
    and open the estate step 0 created, whose library is its own. Packs → "Review a pack" → "Choose a pack…" →
    the copy. The toast reads "the pack does not clear the bar yet" with its errors
    counted, and the drawer opens with them — not formatted and the membership among them
    — and the notes, each from `satz review-pack`. Click the membership in the drawer: the window
    is on Packs with the membership's line marked in the pack's text. The Upstream card
    says the hand-over is manual until satz ships contribute-pack, the clean-review
    line in the error colour; "Copy the path" puts the path on the clipboard, "Open
    the folder" opens the scratch folder. The Private card names
    `<presets_dir>/team-access.local.satz`; "Place in the library" → the toast names
    the file, the review now names the placed file, and the Private card says it is the
    library's own. Edit the scratch copy, review it, and place it again: refused, the
    fork in the library holds other text. "Close": the card's review and the drawer's
    review findings go. Review `vendor/satz/presets/organization-budget.satz`: "the
    pack clears the bar", the drawer stays closed.
