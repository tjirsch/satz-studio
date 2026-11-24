# satz-studio UI

The window: what it is built of, how the parts talk, and which UI component of the Dioxus
component library each part is. The code is `crates/satz-studio/`; the library's
components are `src/components/`, the app's own parts `src/widgets/`, and the layout is
`assets/app.css`.

## 1. Structure

```
App (src/app.rs)            the stores, the app coroutine, the library's theme, the layout
│                           stylesheet, the typeface, the ⌘K listener on the window
└─ Shell (src/shell/)
   └─ EstateHost             one per open estate: owns the estate coroutine
      └─ Frame               the library's SidebarProvider
         ├─ AppSidebar       the primary destinations; Agent and Settings at the foot
         ├─ TopBar           the sidebar's fold, estate name and directory, reload, switch, close; the estate's status
         ├─ SatzBanner       satz missing, too old or not running; a notice while newer than the build
         ├─ Content          the view of `AppStore.nav`
         ├─ PathBar          Back, the next step's purpose and Next, while a step of the path shows
         ├─ DiagnosticsDrawer
         ├─ CommandPalette   over the window while `AppStore.palette_open`
         ├─ NoticeDialog     the first notice the estate holds, while `estate.notices_open`
         └─ ToastHost        what the stores report, as the library's toasts
```

`src/main.rs` opens the window (1280×840, at least 900×600) and initializes tracing from
`RUST_LOG`. `App` loads `Settings`; a settings file that does not parse is a full-screen
refusal naming the file, never defaults.

### State

`src/state/mod.rs` holds one `#[derive(Store)]` root, `AppStore`, provided at the root
and read through the generated accessors (`app.nav()`, `app.estate().model()`),
so a log line re-renders the log and not the sidebar:

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
| `palette_review` | the palette opens on its pack review rather than its first command: a finding of the reviewed pack was chosen in the drawer |
| `toasts` | what waits to be shown as a toast (`VecDeque<Toast>`); `ToastHost` takes each and hands it to the library's toasts |
| `errors` | every error the window reported that is not a finding of the estate already, in full, newest last and at most 50 (`ERRORS_KEPT`): `toast` adds one for each error, `toast_finding` adds none, the error toast is only their count, and the Findings drawer lists them until "Clear the window's errors" or until an estate closes or opens (`close_estate`) |
| `drawer_open` | the Findings drawer |
| `create` | the `CreateStore`: the one `satz init` run behind the Create door — `log`, `running`, `command`, `outcome`, reset when a run starts. What `init` derives from the sign-in is in these lines while the window shows them and in the estate satz wrote; it reaches no file of the app's own |
| `import` | the `ImportStore`: the one `satz import` run behind the Import door — `log` (the `satz init` that runs first on the two-step path included), `running`, `command` (one command line, or the two), `outcome`, and `report`, satz's own import report split out of the run's lines (`ImportReport`); what a live import prints of what it read as the person signed in stays in these lines and in the estate satz wrote |
| `estate` | the `EstateStore`: `model`, `cst` (the main file's document tree as read at the last reload — the views slice a value's source text and a line's text from it), `questions`, `interview` (what the last `satz_interview` call returned; `rename_to` is read from it), `diagnostics` (the findings the drawer lists), `command_log`, `running`, `last_command`, `outcome`, `loading`, `hcl` (the `HclState` of `hcl_dir`: whether `main.tf` is there and whether it has been initialized), `work_tree` (whether git holds the estate file's directory in a work tree — `None` until the first reload has asked), `export_formats` (the formats `satz questions --help` lists, read when the session opens, or why they could not be read), `last_export` (the format and the file of the last export, which "Export again" writes over), `review` (the pack the palette's pack review judged last — the bytes it judged and satz's report — or the pack and why its review failed; a reload leaves it, and the drawer shows its findings until it is closed), `reviewing` (a review or a placement is running), `interfaces` (what the estate publishes, from `satz interfaces` at every reload, or satz's reason it could not say; `None` until the first reload), `notices` (the notices the window holds — what `satz_interview`, `satz_add_preset` and `satz_merge_presets` opened and the estate has not acknowledged), `notices_open` (the notice dialog is raised), `needed` (what a workload estate's resource types need from the organization's team, as `satz_update_prerequisites {report_only: true}` lists it, or why it could not; `None` until "Needed roles" asks and while it is read again) — reset when an estate opens or closes |

`DiagnosticSelection(Signal<Option<Diagnostic>>)` is a second context: the drawer sets
it when a row is clicked, the estate views read it.

### Coroutines

Every side effect that touches an estate or the satz the app runs goes through one of
two coroutines; the stores are written from there and nothing blocks in an event handler.
The exceptions write no estate file: the Agent view runs `satz mcp-config` itself (below),
and the Start screen, the review card and Settings open the OS's file dialogs and folders
from the view.

- **The app coroutine** (`src/state/app_actions.rs`, `AppAction`) locates satz at
  startup (`SatzBinary::locate` with the Settings path), walks `last_root` if there is
  one, and then serves `LocateSatz`, `Discover(folder)` (the walk, the config parse and
  each estate's `deployment_mode` on a blocking thread; the folder becomes
  `Settings.last_root`), `OpenEstate { config, estate }` (`EstateSession::open`, at the
  window's own ceiling, `read,write`), `CloseEstate`, `CreateEstate { dir, options }`,
  `CancelCreate`, `ImportEstate { dir, options, init }`, `CancelImport`, `CancelUpdate`
  and `SaveSettings` (the file, then satz again; no client configuration is rewritten).
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
  `CreateEstate` checks that the target is there, runs `satz init` in it through `SatzCli::run_in` —
  with the interface folders after `--from` and an optional `--name` when the form's
  "A team, from interface folders" is chosen, which sends none of the organization's
  flags ([ADR 0029](adr/0029-presets-and-packs-are-two-destinations.md)) —
  from a task so the loop stays free for `CancelCreate`, streams the lines into
  `create.log`, and then reads what the run left with `init::created`: one estate is
  opened, none is the outcome saying `init` had no customer id to name a file after, and
  a non-zero exit carries satz's first `error:` line whole (`init::refusal`) rather than a
  status number.
  `ImportEstate` decides from the directory whether `satz init` runs first
  (`import::plan`), checks the source, runs the one or two commands from a task so the
  loop stays free for `CancelImport`, streams both into `import.log`, and opens the one
  estate the run wrote (`written_since`). `CancelUpdate` stops a running `UpdateSatz`.
- **The estate coroutine** (`src/state/estate_actions.rs`, `EstateAction`) is started by
  `EstateHost` with the session and lives as long as the estate is open. On start and on
  `Reload` it reads `hcl_dir` into `estate.hcl`, asks git whether the estate file's
  directory is in a work tree (`WorkTree::read`) into `estate.work_tree`, calls
  `satz_questions` and `satz_presets` over the session, `satz interfaces` through the CLI
  into `estate.interfaces` (no MCP tool serves it; a failure is satz's reason there and a
  finding of source `interfaces` in the drawer),
  reads the main file, parses it, resolves the params and loads the schema on a blocking
  thread, builds the `EstateModel`, and writes the model, the questions and the
  findings. A reload that built a model then runs `satz_transpile_check` and adds
  what the transpile found — a prerequisite the estate does not declare, a required
  argument the provider wants, raw HCL nobody has reviewed — as findings of source
  `check`. It is skipped when the front end already refused (the refusal has said why,
  and the check would say it twice) and when the reload follows a write, whose own check
  has just run and whose findings are carried in.
  `RunCommand(args)` runs `satz --config <dir> <args…>` in a tokio task and streams its
  lines, ANSI stripped, into `command_log` from a local task, so the loop stays free for
  `CancelCommand`. A `RunCommand`, a `RunNoticeCommand` or an `InitRepository` refused
  because a command is running leaves that command's cancel token in place, so
  `CancelCommand` still stops it. `OpenInTerminal(args)` writes the one-shot script
  (`EstateSession::external_command`) and opens it in the OS terminal. `Close` drops the
  session and puts the window on the Start screen as it stands; `Switch` does the same
  with the Open door showing.
  `ReadNeeded` is the Overview's "Needed roles" pressed: `needed` is cleared and
  `satz_update_prerequisites {report_only: true}` read into it in a task of its own —
  the report typed as `PrerequisitesReport`, a report without `workload`,
  `needed_roles` and `needed_apis` a failure the dialog shows, never an empty list.
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
    restores the bytes, puts its findings in
    the drawer — they stay through the reload that follows — and an error toast counts
    it; a check that passes puts the findings it reported there instead, so a
    warning shows at its line beside the write that landed. `AcceptDefaults` is the
    same call with `accept_defaults: true`.
  - `CommitEdit(Edit)` is the app's writer: `EditSession::open`, `apply(&[edit])`,
    `Proposed::commit(&McpChecker)`. An edit the document layer refuses (`EditError`)
    is a toast and nothing was written; `Rollback::Check` is findings in the drawer and a toast as
    above; `Rollback::ChangedOnDisk` is the toast "changed on disk — reloaded". A commit
    that lands carries the check's own findings into the drawer, as an answer does.
  - `AddPack(AddPackArgs)` and `RemovePack(RemovePackArgs)` are satz's own pack logic
    under the same delegated-write discipline: `satz_add_preset` binds the pack's gate
    true (an option of a choice sets its siblings false) and uncomments its line, or
    writes it where the pack graph places it — the map's line as much as any other;
    `satz_remove_preset` binds the gate false and leaves the line. A switch satz refuses —
    a pack it needs is off, a pack that needs it is on, a line not gated on its gate —
    is satz's sentence in a toast and a finding in the drawer, beside what the
    reload's check says of the file, and the file is compared with its record and put
    back as an answer's is. A switch that lands says what it switched and
    what it opened, and raises the notices it opened as an answer does. The app writes
    no pack line itself.
  - `WritePrerequisites` is the same delegated write over `satz_update_prerequisites
    {report_only: false}`: satz works out offline which roles the estate account
    lacks and which APIs a project its resources' calls are billed to does not enable
    — the infrastructure project, or the project a resource sits inside — and writes
    both into the estate, naming rather than writing an API for a project a pack
    declares; in a workload estate it writes APIs on its own projects alone, its roles
    being the organization's to grant. `delegated_write` is the one implementation the two share.
  - `RunNoticeCommand(args)` is the command a pack's notice names — `satz adopt
    <estate> --execute --import`, which puts the live ids into the estate file and binds
    the notice's param — as a delegated write whose call is the CLI: the lines stream into
    `command_log` while it runs, the main file is recorded before and checked after, a
    non-zero exit is satz's refusal with the bytes put back where satz had changed them,
    and `CancelCommand` kills the child. The outcome under the log says whether it landed
    and the check passed, and the toast is the last line satz printed.
  - `MergePresets` calls `satz_merge_presets`, which reconciles the estate's library
    with upstream. The log gets the report as sentences (`MergeReport::lines`): one per
    pack the merge changed or would change, satz's warnings, notes and prerequisites,
    the counts, and each notice it opened with the command that notice names. A report
    with `attention` — what makes `satz merge-presets` exit non-zero — is a failed
    outcome and a toast, as a command that exits non-zero is. A refusal is satz's own
    text; a report the app cannot type is a failed outcome and a toast.
  - `ReviewPack { pack, against }` runs `satz --config <estate dir> review-preset <pack>
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
    refuses removes the file again, and its findings join the drawer.
  - `CloseReview` drops the review, and its findings leave the drawer.
  - `Packs(PackOp::Publish { id, source, force })` runs `satz publish-pack <id> --source
    <source> --estate <the open estate> [--force]` under the write lock
    (`workload::publish`); the report is a toast (`published_text`) and, like the
    refusal in satz's words, is kept in `pack_publish` for the dialog; then the list is
    read again from the listed source, the published one when none was listed.
  - `NewPack(id)` writes a new pack of the repository's own under the write lock
    (`workload::create_pack`: `packs_dir/<id>.satz`, `pack <id with _> version "0.1"`
    and an empty `params` block, formatted by `satz fmt`, refused when the file is
    there), then adds the `use` line satz names for it (`EstateDir::use_path_of`) as
    `CommitEdit(Edit::AddUse)` does, then opens the file with `edit_file` — the Editor
    setting's command, or the system's text editor when it is empty ([ADR
    0033](adr/0033-a-pack-opens-in-the-editor-setting-or-the-systems-text-editor.md)).
    A refused line leaves the pack where it is and opens it all the same.
  - `Packs(PackOp::Update { which, from, take_upstream })` runs `satz update-pack`, with
    `--source <from>` when the Manage dialog asks it for one source's packs; that source
    is listed after it.

An error from either — a refusal, a missing binary — is counted in an error toast and
listed in full in the drawer, and where it concerns the estate it is also a finding there
or an outcome under the log.

### Views

The window is ordered by the job. With an estate open the
sidebar reads **Overview, Presets, Packs, Decisions, Apply, Security, Documents, Estate**, then Agent and
Settings at its foot; with none open it carries Settings alone and the window stands on
the Start screen. Presets and Packs stand before Decisions because a pack is what DECLARES a
question: a pack switched on opens the questions it asks, so the questions
Decisions lists are what the two have just let in. An estate created here
lands on Decisions rather than Overview — a skeleton has nothing else yet — and every
other door lands on Overview; the sidebar's order is the same for all of them. An estate is
an organization's estate or a workload estate, as satz's presets report names it
(`works_in`: `organization` or `workload`) ([ADR 0032](adr/0032-an-estate-is-an-organizations-or-a-workload-estate-and-a-pack-shows-its-declarations.md)).
Presets is the destination of an estate that holds the preset library; Packs is every
estate's ([ADR 0029](adr/0029-presets-and-packs-are-two-destinations.md)). A workload estate has no
Presets destination. All of the sidebar but Estate is the path through an estate, and while one of their views shows, the path bar stands at the window's foot (`PathBar` in `src/views/path.rs`, `.path-bar`; the anatomy is under Shell); Estate stands after them and has none ([ADR 0028](adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)). The kind is a row of the
Overview's identity card, "Kind", one badge — "Organization's estate"
(`Building2`) or "Workload estate" (`Package`) — and no sentence, with the
interface folders a workload estate reads under "Delegated by". Estate has three tabs for every
estate. Apply offers a workload estate its own steps — `transpile --check`,
`update-prerequisites`, `transpile`, `hcl-init`, `plan`, `apply` — and names the bootstrap
and the state's move as the organization's;
Documents offers the compliance commands where the preset library's catalogs are.

**Decisions and Presets/Estate are not duplicates, and this is the rule that says so:**
Decisions is the WORKLIST — what the estate has not decided yet, one question at a time,
with what each costs to change later. Presets and Estate are the STATE — which packs the
file runs, which params it binds, which resources it declares. A param that answers a
question is editable in both: the question is where the choice is explained,
the file is where it lives. It is the same rule as "one fact, one place" — a param that
is a pack's gate in satz's pack graph shows in Presets and not in Estate, because there it
is a pack switch and not a value.

| view | file | what it does |
|---|---|---|
| Start | `src/views/estates.rs` | the way in: a row of three door cards (`state::Door`) over the pane the chosen door opens. **Open** is the folder (typed, or picked with the OS dialog), one card per `config.toml` with its estates, each with its deployment mode and an Open button; the open estate is marked and can be closed. It is where the window stands with no estate open, and where closing one returns it |
| Create | `src/views/create.rs` | the **Create** door: the folder the new estate goes in, picked or typed, refused while it is not there or its `config.toml` does not parse; a folder that already holds an estate's files (`init::existing`: `config.toml`, `.gitignore`, the estate files of its `yaml_dir`, an earlier copy of an interface folder under `interfaces/incoming/`) gets one warning line under the field — "This folder already holds an estate's files: …", and what satz does with each kind — and a `Switch` "Overwrite existing files", off by default and not drawn over a folder that holds none, which adds `--force` to the run; Create stays enabled either way; what the estate is for as a toggle group, "An organization" or "A workload, from interface folders"; for a workload estate, the interface folders a delegation was given (`--from`) and an optional name (`--name`); for an organization's estate, "What satz cannot derive" (customer shortname, default region), the Terraform tool as a toggle group, the Google provider set as a switch, extra providers as a tag list, and, under "From your sign-in, unless you state them", the customer id and the billing account as overrides, empty by default and each saying which live call answers it when it is left blank — the workload folder is no field: the interview asks `workload_folder_name` under Decisions; the command line as it will run; Create and Cancel; and the run log with stdout and stderr distinguished, a line saying what satz derives is printed there and kept nowhere else, and the outcome: a badge naming the estate opened, or, when the run refused, satz's whole first `error:` line (`init::refusal`) wrapped under the log's head |
| Import | `src/views/import.rs` | the **Import** door: the folder the import runs in, picked or typed, with a line saying which of the two things will happen — `satz import` alone, or `satz init` first because the folder holds no `config.toml`; the source as a toggle group over the three shapes (state document, live scope, Terraform HCL) with the source field and its picker below, each shape saying in its supporting line what it reads — the state one that it is `tofu show -json` output and not a raw `.tfstate`; then the flags of that shape alone (only/exclude/all, the collision rule, the customer shortname, the output file and `--verbose` for state and live, and the organization for the state and HCL shapes — `--organization`, which satz needs to write an estate from a state or a configuration that names no organization, and from every wrap-all import; wrap-all for HCL); the Terraform tool and the provider-schema switch when the init half runs; the command line, or both of them, as they will run; Import and Cancel; and, beside the log, satz's report split into what it wrote, what it skipped, the params it could not derive and its warnings — the attributes an apply would reset and the asset types Cloud Asset Inventory does not serve among them — with "Check it transpiles" running `satz_transpile_check` on the estate that opened |
| Overview | `src/views/overview.rs` | the identity card first — which estate this is, by its own answers: `identity()` opens with Short name, Customer, Customer ID and Organization ID (`customer_shortname`, `customer_longname`, `customer_id`, `customer_organization_id`) read from the model's `params` — the file's own `params { }` block, whether or not the estate uses the `estate_core` pack — each reading its value, "not set" where the file does not set it (never a pack's default), "empty" where it is `""`, and "not read yet" until the model is built; then the other rows the `estate_core` pack declares, out of the questions report, in a written order (Domain, then the infrastructure, the estate account and its estate account user group, the engine and the region), a core subject the order does not name under a label made from its own name, an unanswered one saying so rather than showing the pack's default, and `deployment_mode` left to the State row below it — the four subjects above are never a question row, so each shows once; then the Kind row, one badge, "Organization's estate" (`Building2`) or "Workload estate" (`Package`), from `works_in` of satz's presets report and no sentence; for a workload estate the Delegated by row, one line per interface folder it uses (`delegated_by_lines`): the folder's name in monospace, the organization's estate that wrote it, the estate account with who may act as it and where its calls are billed when the folder carries one, and "exports" with the first five names (`EXPORTS_SHOWN`), then "and N more" (text `Button`) opening a `Dialog` titled with the folder that lists every export ([ADR 0032](adr/0032-an-estate-is-an-organizations-or-a-workload-estate-and-a-pack-shows-its-declarations.md)); for a workload estate the Prerequisites row, "Roles are the organization's to grant" and "Needed roles" (text `Button`), which sends `EstateAction::ReadNeeded` — `satz_update_prerequisites {report_only: true}`, offline — and opens a `Dialog` titled "Needed roles": a `Progress` while it reads, satz's refusal in the error color, or the estate account the roles are for, the roles under one heading per place they are granted (`needed_by_scope`: on the project, on the folder, at the organization, on the billing account, in the Google Workspace admin console), each alternative in monospace with the resource types that need it, then "APIs on the delegation's infrastructure project", each API with its project and the types that need it; "Copy" (secondary) puts `needed_text()` on the clipboard — the account, then one line per role and per API with where it is granted or enabled and what needs it — and "Close" (ghost) lowers it. A workload estate's roles are not checked by satz (its ADR 0101), so the list is information for the organization's team and never an item of "Still to do"; then the Runs as row, `runs_as_row()` over what `satz_open` reported, telling apart the cases `satz whoami` does — the estate account impersonated by the signed-in identity in cloud mode, the signed-in identity itself in local mode with the `satz migrate --mode cloud` that switches it, and the signed-in identity in cloud mode because the estate declares no estate account; then the file, directory, state, schema and HCL rows, and a badge counting the rows without a value — not answered, not set or empty. Below it "Still to do": what the estate owes, derived on every render by `owed()` over `Facts` — the estate not in a git repository (so `satz merge-presets` refuses and no preset update can land; git's own words, and "Create the repository" running `InitRepository`) or git not installed (said, with nothing to press), the bootstrap unconfirmed ("Check the bootstrap"), questions unanswered, the estate map off or absent, the pack graph's findings about the packs the estate uses (the first one's sentence, and Presets as the destination), the pack lines with no gate that no finding covers — a `use` line without its `when <gate>` switches the pack on whatever the estate answers — no provider schema, packs whose notice is open — counted from the transpile's own `notice` findings, with "Show the notices" where this session opened them and nothing to press where it did not, the drawer carrying each with its command and its param — prerequisites the transpile found undeclared ("An API is not declared" for a workload estate, whose roles satz does not check), raw HCL without a `trust` reason, an HCL directory that has not been transpiled or not been initialized — each row with what to do about it: a destination, a command run here, or a terminal hand-off. That card is not drawn when the list is empty — the top bar's status button says that nothing is owed; below it, once something has run, the shared command log follows. For an estate that reads interface files the identity card has a row "Delegated by": one sentence per file from `delegated_by` of satz's presets report — the organization's estate, the estate account and its estate account user group, the project its calls are billed to; a delegated resource's file names its project or says it is a folder (`delegated_by_lines`) |
| Decisions | `src/views/interview.rs` | the questions report one question at a time, unanswered first with a "Show answered" switch: the pack's description when the pack changes, the prompt, the `why`, badges for reversal and blast and a one-way-door badge — each badge of the card, the state badge included, with a tooltip that says what it means (`state_tip`, `reversal_tip`, `blast_tip` and `ONE_WAY_TIP` in `src/views/interview.rs`, in satz's words for `reversal` and `blast`: an edit is one line, one apply; state surgery is a rename plus `state mv`; a recreate destroys and recreates resources; blast is what the change costs the running organization) — the recommendation when it differs from the offer, the field in the shape `answer_kind` reads off the report — the `shape` the pack declares the param with, else the offered value's, which is the order satz's `parse_answer` reads an answer in, so a param a pack declares `[]` offers nothing and is still a list — as a switch, a number field, a tag list or a text field that refuses a brace with satz's sentence; a tag list takes several values typed at once with commas. A question whose param is declared as a map, or with no shape and nothing offered, shows no field and says the value is written into the estate's params by hand, a `oneof` as buttons with the chosen option's `why`, starting on the option the estate binds, else the pack's default; a `oneof` that is not `required` has a **None** badge after its options, which answers `none` — every option bound `false` — and which the card starts on when the estate binds no option of an answered choice or satz offers `none`. A question whose `empty` says what `""` means takes an empty field as its answer: Accept is enabled on it, and the field's supporting line, the card's state badge and the list say the meaning beside `""`. The card has one primary button, and it reads **Accept** (`Check`) or **Next** (`ArrowRight`) (`primary` in `src/views/interview.rs`): on a question not answered it reads Accept whether or not the value was changed — accepting the offered default writes it — and is disabled while the field has a problem, while it is empty on a question that offers nothing, while no badge is chosen, and while a write is in flight; on an answered question, or one not asked, it reads Next while the field holds the written value, or the badges the bound option, and Accept as soon as the value differs — putting the value back returns it to Next. Accept writes the card's value (one `Answer` action) and moves to the next question; Next moves and writes nothing. Changing the field or picking a badge writes nothing: a keystroke, a switch flip and a badge added to or removed from a list only change what the card holds, and the field losing focus writes nothing either (`ON_BLUR` and `ON_CHANGE` in `src/views/interview.rs`, over `commit_on_blur` and `commit_on_change` of `TypedField`) — Params and Resources keep saving on blur, on a flip and on a list change, where that saves a value into the file being edited. Enter in a text or number field is the keyboard form of the primary button: it does what the button reads, and nothing while the button is disabled. Beside the primary button, the ghost button **Skip** on a question not answered, which moves on without writing; an answered question has no Skip. A question with no field has no primary button: its ghost button reads Skip, or Next where the question is answered or not asked. **Back**, a ghost button, returns to the question last left, switching "Show answered" on when that question is answered. The card follows its question by subject: Accept moves the walk over the report as it stands before the write, so the reload that takes the answered question out of the unanswered block leaves the card on the question after it — not one further, not back at the top — and a question that leaves the walk hands its place to the one that took it. A reload that changes the value a card starts on starts that card afresh (`card_key`). Accepting the last open question empties the walk, and Back returns to it. Back, Skip and the primary button stand in an action bar below the question, outside its scroll: the pack header and the card scroll above it, the bar stays in view and at the same place on every question — Back at the leading end, Skip and the primary button at the trailing end, on the theme's surface (`.interview__actions`, `.interview__scroll` in `assets/app.css`), spanning the question's column; "Accept n defaults"; the list beside the card always, never only with the switch on — the switch decides what is IN it, unanswered alone or every question — each row its prompt, subject and answer, a click putting that question on the card, and the one on the card selected — tinted with the theme's focus blue and outlined in it, in light and dark (`.interview__list [data-slot="item"][data-selected="true"]` in `assets/app.css`; the library's `Item` has no selected variant), and, where the filter leaves nothing, the row that says so ("No unanswered question…"). The view fills the content area and does not scroll: the walk scrolls and the list scrolls, each to its own height, so a long list never moves the question; the walk takes two thirds of the width and the list one at every window width, and the question's text flows across the whole of its card — in a window under 1100px the two stack, the view scrolls as a whole again, and the action bar sticks to the bottom of the window while its question is in view. The progress bar carries the count and the completion in one line ("n of m answered · nothing open — bootstrap and apply will not refuse this estate for an open question"): the bar is the one counter — no separate card says every applicable question is answered, and the question card carries no second count. Then the `rename_to` card when the last `satz_interview` returned one. |
| Presets | `src/views/map.rs` | the destination of an estate that holds the preset library ([ADR 0029](adr/0029-presets-and-packs-are-two-destinations.md)): satz's pack report (`EstateModel::packs`, from `satz_presets`) and nothing the app derives. The map first: a badge with its line and "Run merge-presets" beside it while it is switched on, else a card saying its line is commented out (with the line) or absent, with "Switch the map on" (`AddPack`). When the presets carry no pack graph, satz's `note` is a card of its own. Then every pack whose line the file carries, in the file's order under the phase its line stands under (the phase comment's first line, from `EstateModel::phases`; a line without one joins the section open at that point), then every pack the file has no line for under "Not in this file", then the `use` lines the graph does not know under "Not in the pack graph", each a card with its path and line and no switch. A pack's card: its path and a badge with its line state — active, ungated, commented out, absent, forked or misplaced, with the line number, a filled circle when the pack is switched on, and destructive for ungated and misplaced — the question that asks its gate, where the questions report has one, the gate as satz reports it (`use_budget = true (answered true)`, or the default), the fork the line names, a line gated on another param, and a switch bound to whether it is switched on: on is `AddPack`, off is `RemovePack`. The core pack and a pack whose line is written by hand carry no switch and say why. Below, what it needs, each with whether it is met — every requirement that is not, and the met ones the tree does not already draw — with "Switch on" (`AddPack` of that pack) on one that one pack meets; what needs it; what it excludes; its notices with the command and the param that acknowledges it; and the transpile's findings about it, each with the command satz names for it. A card whose line is ungated or misplaced and that carries no finding says what that state means and the edit that answers it (`line_note`): an ungated line names its gate, the `when <gate>` to write on it and the comment that takes the pack off; a misplaced one says the line stands outside the block the pack graph places the pack in. satz reports `ungated-pack` only while the file declaring the gate is switched on, so with the map off that sentence is the only word about a line no answer switches off — and where satz does speak, its finding stands there instead. A finding about no pack the report has a row for stands above the sections. The requirements make a tree of the same cards (`sections(report, phases)`, a pure function): a pack hangs below the pack that alone meets its first requirement (`parent_of` — a requirement several packs can meet, the billing permissions' choice of a security-group model, hangs a card nowhere, and neither the map nor the core pack is a parent); a card nothing hangs from and that hangs from nothing stays a cell of its section's grid; a card others hang from is a block across the whole grid row — a `ConnectorTree` with the card at the top and the packs that need it below, in section order, recursing (the audit-log archive → the central alerts → the findings mail is three levels). A child hangs under its parent wherever its own line stands, and leaves its phase: a section left with nothing is not shown, and a child whose phase is not its parent's carries that phase's first line as a caption above its card. A connector is solid for a gate or a `requires` requirement and dashed for a data one — the child reads params the parent declares — whose caption names them ("reads `…`"); where the child is switched on while its requirement is not met, the connector and the trunk leading to it are in the error color and the caption reads "switched on while `<pack>` is off". The title carries a help button at its top-right corner whose dialog holds what a switch and merge-presets do; the lead under the map is two lines. A pack card's question `why` is behind a help button at the card's top-right corner. The pack review is not here: it is an entry of the commands palette ([ADR 0031](adr/0031-the-pack-review-is-in-the-palette-and-a-new-pack-opens-in-the-systems-editor.md)) |
| Packs | `src/views/packs.rs`, `src/views/workload_packs.rs`, `src/views/pack_sources.rs`, `src/views/publish_pack.rs` | every estate's workload packs ([ADR 0029](adr/0029-presets-and-packs-are-two-destinations.md), [ADR 0030](adr/0030-pack-sources-are-satzs-and-the-window-adds-them-through-satz.md), [ADR 0031](adr/0031-the-pack-review-is-in-the-palette-and-a-new-pack-opens-in-the-systems-editor.md), [ADR 0032](adr/0032-an-estate-is-an-organizations-or-a-workload-estate-and-a-pack-shows-its-declarations.md)): a two-line lead that says what a pack is — a template to take and adapt —, then up to three cards, each with a help button (`HelpButton`) at its top-right corner whose `Dialog` holds the rest of what the card means. **Pack sources** is one line, `sources_summary` of `satz pack-source list` read when the view first stands ("1 pack source: satz", "3 pack sources: satz, team, …") and the source listed, with "Manage" (text `Button`) opening a `Dialog`: a two-line `List` of every source — a leading cloud or folder icon, its name as headline and where it is as supporting text, the built-in `satz` first and marked built in —; choosing one runs `satz packs --source <name> --estate <the open estate>` — a GitHub source is a network read, never made unasked — and closes the dialog; each row's trailing icon buttons update the packs fetched from it (`update-pack --all --source <name>`) and, for a configured source, remove it after the confirming `Dialog` (`satz pack-source remove`); "Add a source…" turns the dialog into the form — a `Field` for the name, a two-item `ToggleGroup` "GitHub repository" / "Folder", then the repository, "Branch or tag" and "Folder of the repository" text fields, or "Choose a folder…" with the OS dialog and the chosen path — whose Add runs `satz pack-source add` and returns to the list on satz's write; a refusal stands in the dialog in an error block as satz says it. **In this estate** lists the packs of `packs_dir`: first every one the estate's active `use` lines read (`EstateDir::pack_of_use` over the document tree), in their order, then the other files a listing names (`here_rows`). When the view first stands, and after every pack command — a list, a fetch, an update, Keep as own, a publish, New pack — `satz packs --local --estate <the open estate>` lists the packs here (`PackOp::ListLocal`, `workload::list_local`): no pack source is read, so nothing goes over the network, and every row has its state, its origin, its badges and "Details" at once; a listed source's row wins for every pack its report names, the local one stays for the rest. Its head carries "Update all" — the card's one secondary button — while a pack has an update available, "New pack…" (ghost), a `Dialog` with a `Field` for the name, checked by `new_pack_id` with the problem as the field's supporting text, whose Create sends `NewPack`, and "Reload" (`RefreshCw`, icon button), which reads the estate again (`EstateAction::Reload`) and the packs — the listed source's list where one is listed (`PackOp::List`), the local one otherwise (`PackOp::ListLocal`), the local one after either — the way a pack written in an external editor after New pack or Edit is seen, since the window cannot tell when the editor is done with it; it is a button and not the window's focus because a focus would run `satz packs`, a network read for a GitHub source, on every return to the window. **Available** lists what the listed source holds beyond them. A row is one line: the id, the version (the source's beside it where it differs), one badge where the state needs attention (`badge`: "Update available", "Edited here", "Gone from source" in the error color, "Not checked" for a pack fetched from another source than the listed one, "Unchanged since fetched" for a fetched pack of the local listing — its file is the one fetched, and its source was not read; none for a current pack or the repository's own), "Unmet read" (`TriangleAlert`, error color) where a read of the pack is neither given by an interface folder of the estate nor set by the estate (`WorkloadPackRow::has_unmet_read`), "↓ N in · ↑ M out" on every row satz listed — fetched or only available — the disclosure of what flows through the pack, seen from the pack: the trigger of the library's `Collapsible`, rendered as a small ghost `Button` with `ArrowDown` before the count of the distinct values the pack reads (`folder.value` or `value`) and `ArrowUp` before the count of its exports (`flow_counts`), and a trailing chevron. It opens the collapsible's content inline under the row (`PackFlow`, `.wpacks__flow`): a block on the theme's surface holding two sides side by side where they fit and stacked where they do not, each a heading with its arrow over groups, each group a source line over a list of two-line entries. **Incoming** (`incoming`) has one group per interface folder that meets a read (`folder_shared`, the folder in monospace and "written by <estate>" from the report's `interface_folders`; a read several folders meet stands under the first of `met_by`), in the order the pack first reads from each, then "set by the estate" (`SlidersHorizontal`) for the reads whose param the estate sets, then "not given" (`TriangleAlert`, error color) with satz's reason under each value; an entry is the value in monospace with the description of the export that gives it, and under it "→ <param>" with the question's prompt for each param the read fills, the question's `why` below in supporting text — a value read for several params is one entry. **Outgoing** (`export_groups`) has a group per interface, "core" first, then "interface <name>" in the order the pack names them (`ArrowUp`), each export in monospace with its description. A side with nothing says "nothing". "Details" (ghost button) on every row satz listed opens a `Dialog` titled with the pack's id (`PackDetails`, `.wpacks__details`) in two sections under headings: **Params** (the name, its default in monospace or "no default", "asked" with the question's prompt where a question asks for it, and the question's `why` under it) and **List params** (the param, its key, its fields, the defaults of fields), each "none" when empty; where it comes from ("Own pack", or the source with the short commit of a GitHub source), and its actions — Fetch, Update and Add to the estate (`Edit::AddUse`) as text buttons, Show the difference, Take the source's file…, Keep as own… (the last two behind the confirming `Dialog`), Store in a pack source… (`FolderUp`, on an "Own pack" row alone) and Edit as icon buttons; Edit opens the file in the Editor setting's command, or the system's text editor when it is empty — `open -t` on macOS, `xdg-open` on Linux, `notepad` on Windows — with `..` resolved out of its path and without waiting for the editor (`edit_file`, `satz_studio_core::editor`, [ADR 0033](adr/0033-a-pack-opens-in-the-editor-setting-or-the-systems-text-editor.md)); a command that does not start or exits with a failure is the toast "<file> did not open with <command>: <error>. Set an editor in Settings → Editor." Store in a pack source… opens a `Dialog` (`PublishDialog`): a supporting line saying what `satz publish-pack` does, then a `Radio` per folder source of `satz pack-source list` (`folder_sources`: name and folder; a GitHub source and the built-in `satz` are not offered), the first chosen; Cancel (ghost) and Store (primary) run `PackOp::Publish`. With no folder source the dialog says so and its secondary "Add a source…" closes it and opens the pack sources' add form. satz's report closes it, and the toast says what it did (`published_text`); a refusal keeps it open in an error block — review-pack's bar as its error lines and the `satz review-pack` command that lists every finding, anything else as satz said it. Another file of that id in the folder adds "Overwrite…" (ghost), which shows a line naming that file and turns the actions into Cancel and Overwrite (primary), the second run with `--force`. An edited pack's difference is a monospace block, `wpacks__diff`, under its row |
| Estate | `src/views/estate.rs` | the estate in three tabs, `ParamsPane`, `ResourcesPane` and `InterfacesPane`, with the counts on each — the interfaces' count is the interfaces the estate declares, `…` before the first reload and `!` when satz could not say. A finding chosen in the drawer opens the Resources tab, where its line is. The Interfaces tab is a tab and not a destination. A block or an attribute satz writes under a condition carries it as a badge, `when use_key`, in the tree and on the row: the value is edited as any other, the condition is read. |
| Estate · Params | `src/views/params.rs` | one row per `ParamRow`, grouped by the asking question's pack (else "estate"): a typed field by `ParamKind` in value mode, or the Satz source in source mode — a row whose value carries a `{param}` or `${…}` opens there, with its parts as badges; the question's `why` as a tooltip, a one-way-door badge, a raw-line toggle showing the line; a commit on Enter, blur, a switch flip or a list change is `CommitEdit(Edit::ReplaceParam)` with a `TypedValue` in value mode and `TypedValue::Raw` in source mode |
| Estate · Resources | `src/views/resources.rs` | two panes: the tree of `ResourceNode`s (an icon per kind, a resource's name, `use` lines as leaves, branches collapsed below depth 2, a badge with the count of required attributes not written) and the selected node's card: kind, type, line, the missing required names, then one row per `AttrRow` — a typed field by `AttrType` (string, number, bool, a list of one of them; everything else and `Unknown` in source mode), locked rows dimmed with the reason (`import-id`, computed, not in the schema), source mode with its badges; a commit is `CommitEdit(Edit::ReplaceValue)`. Without a schema every row is locked and the header carries "Run update-schema". A row clicked in the drawer selects the node at its line |
| Estate · Interfaces | `src/views/interfaces.rs` | what the estate publishes for other estates to read, as `satz interfaces` reports it, and nothing the app derives: a lead line saying what is shown, where a transpile writes each interface folder and how delegated access is given — an entry of the list param `delegations` of `presets/delegation.satz`; for an estate that declares no export and no interface, one card saying it publishes nothing; else an card **core** — "every interface carries these" — with the core exports; then one card per declared interface in the report's order: its name, a `common` badge (`Globe`) with "In the library every project's folder carries" where satz marks it common, its `file:line`, a "uses" row of badges (`Waypoints`) naming the interfaces it uses, and its own exports, or a line saying it declares none and carries the core exports and those of the interfaces it uses; each interface that is not common has a ghost button "Show folder", which opens its outgoing interface folder, what the last transpile wrote under `interfaces/outgoing/`, in the file manager. An export is a row: its name, a badge for how an estate reading it holds it — `static` (`Type`), `lookup` (`Search`) or `map · <type>` (`Braces`) for an `all` map — a badge `attach <type>` (`Link`) per attach point, its `file:line` at the row's end, then the value in a monospace block and its description. `file:line` is text: an export line is no resource node, so it selects nothing. Last, when the estate's packs declare list params, an card **Lists that take entries** (`Inbox`): a line saying a pack declares what an entry of its list is, that "Add" writes one into this estate (`satz add-entry`) and that a workload's team asks for one in a request file, `contributes_<list param>`, checked with `satz check-request`; then one row per list param declared with `entries` — its name, a badge `key <field>` (`Key`), how many entries it holds, its `file:line`, a secondary "Add", the fields an entry may carry in a monospace block — `<field> ~ <regex>` for a field the declaration gives a pattern — its description and its entries as badges. "Add" opens a `Dialog` holding one field per declared field — a `Switch` where the default is a yes or no, a monospace `Field` otherwise, the pattern as its supporting text and the default as the value an untouched field keeps — whose "Add" sends `EstateAction::AddEntry`, satz's `satz_add_entry` under the delegated-write discipline; a field left as its default is not sent (`entry_fields`) ([ADR 0029](adr/0029-presets-and-packs-are-two-destinations.md)). While the first reload runs the tab is a progress card; when satz refuses the estate it is satz's reason in the theme's error color |
| Apply | `src/views/apply.rs` | the estate's way to the organization ([ADR 0028](adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)), from the top: the sign-in card, an `Card` — `satz_whoami` online, sent when the view is entered and no answer is held for the estate and by "Check again" (ghost button): its first line says who is signed in, that nobody is, or that somebody is and the estate cannot run (`sign_in_headline`), then one line with an icon per thing checked — the account the estate runs as, the project its calls are billed to, the permission test (`sign_in_lines`) — or satz's refusal in its own words with the sign-in command and a primary "Sign in" that runs it in the OS terminal (`agent::start`); the state line, a `Card` as one row: the `hcl_dir` path and four badges — transpiles, rights declared (APIs declared for a workload estate, `prerequisites_badge`), `main.tf` written (or not transpiled), initialized; where the transpile found prerequisites missing, the card "The prerequisites are not declared" ("An API is not declared" for a workload estate, whose roles are the organization's to grant and never written here) that lists them and offers "Write them into the estate" (`EstateAction::WritePrerequisites`); then the `APPLY` deck in the order it is walked — `transpile --check`, `update-prerequisites` (`--report-only`, fixed), `bootstrap --dry-run`, `bootstrap`, `transpile`, `hcl-init`, `plan`, `apply`, `migrate` — the three that change a live organization with "Copy" and "Open in terminal" and no Run. While the last sign-in check is a refusal, every command deck says so above its command line |
| Security | `src/views/security.rs` | hints on what the estate declares ([ADR 0035](adr/0035-security-is-a-destination-of-hints-on-the-estates-own-resources.md)): a lead line, then one `Card`, "Checkov scan" — its description counts the last scan (`summary`: Checkov's version, the resources it read, failed, passed and skipped), or says what the scan reads, or, while nothing is transpiled (`HclState::transpiled`), that the transpile on Apply writes it; its `CardAction` is "Scan" (primary) or "Scan again" (secondary), disabled while one runs and while nothing is transpiled, which sends `EstateAction::Scan`: `satz scan <estate> --format json`, Checkov over `hcl_dir`, with an indeterminate `Progress` while it runs. Its content is satz's refusal in the error color, "No check failed.", or an `ItemGroup` with one outlined `Item` per resource in the report's order (`ScanReport::by_resource`): `ShieldAlert`, the resource address in monospace as title, "declared at <file>:<line>" or "not traced to a block of the estate" as description, and under it one line per failed check — an outline `Badge` with Checkov's id, its words, and an extra-small ghost "Guideline" (`ExternalLink`) that opens Checkov's page in the browser. The last scan is held for the open estate until the next one; nothing is written, no sign-in is asked, and no control number is shown |
| Documents | `src/views/documents.rs` | what the estate hands to people as a file ([ADR 0027](adr/0027-documents-is-a-destination-of-its-own.md)): a lead line and three `Card`s, each a `CardTitle` and a `CardDescription` over its content. The first is "Decisions sheet" (`src/views/export.rs`, `ExportCard`), whose text names its two uses — signed before an organization is touched, kept as the record after the apply: a toggle group of the formats the installed satz lists in `satz questions --help`, starting on `markdown` — the decisions sheet — when satz offers it, with satz's own line for the chosen one under it; "Export…" opens the OS save dialog in the estate's directory on `<estate>-decisions.<ext>` and sends `Export { format, out }`, which runs `satz questions <estate> --format <format> --out <file>` into the shared log and opens the file once it is there and not empty; after one, "Export again" writes the same format over the same file and the card names both. A help the app cannot read is the card's text and a toast, and Export is not offered. Under it "Compliance", the `COMPLIANCE` deck — `require` and `report-compliance` — or, for an estate without the preset library's catalogs, the card's description alone saying where its compliance is reported; and "Written by an agent", an `ItemGroup` with one outlined `Item` per skill of satz's (`AGENT_DOCUMENTS`: the estate's documentation, the security review) — its icon, title and text, then the skill's command and a secondary "Start in <client>" that runs `<client> "/<skill>"` in the estate's directory in the OS terminal (`skill_command`, `agent::start`); the app does not check that the skill is installed and reads nothing back ([ADR 0028](adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)) |
| Agent | `src/views/agent.rs` | configuring an agentic client on the open estate and starting it: a lead card saying the app runs no model and that the ceiling bounds the satz server only, with the Settings ceiling as a badge, then one card per client — **Claude Code** and **Claude Desktop**. Each card holds what the client's file holds for the estate — "not configured", "configured: <ceiling>", or another entry in a block in the theme's warning color with "Replace it" — then the block `satz mcp-config <estate> --client <client> --allow <ceiling>` printed, satz's notes under it in the secondary color, and the row "Configure <client>" and "Copy"; the Claude Code card also carries "Open in <client>", which runs the configured command in the estate's directory in the OS terminal. "Configure <client>" runs the same command with `--write`: the line satz ends on is the toast and all of satz's words stand in the card. A refusal stands in the theme's error color in that card, as satz wrote it, and only the one refusal satz answers with `--force` — its own key already there with other arguments — offers "Replace it". A command that is not configured disables "Open in <client>" and says Settings names the client; one that is not installed is an error toast naming it |
| Settings · releases | `src/views/satz_release.rs` | the half of the Settings satz card that is about releases (`SatzReleaseActions`): what the launch looks found for satz and for satz-studio, or why a look failed, or that the satz check did not run and why; "Look for a satz-studio update", one look at a time; and, while no satz is found, "Install satz" with the installer's log. The banner offers the same actions while satz is unusable or newer than the build; here they always are |
| Settings | `src/views/settings.rs` | every `Settings` field as a form, in four cards: **satz** — the path with the detected version, "Update satz", "Check only" and the run's log, `SatzReleaseActions`, and the capability ceiling of the agent's satz MCP server — written into a client's configuration when Configure is pressed on the Agent page, bounding the satz server and not an agent's own shell; the app's own `satz mcp` runs at `read,write` whatever it holds; **Agent** — the command line of the agentic client the Agent destination starts, with where that client is on `PATH` under the field, in the error color when it is not installed or not named; **Editor** — the command line a pack's file opens in from Edit and New pack, the file its last argument; empty, the field's placeholder and the line under it name the system's text editor; set, the line reads "runs <command> <file>", and a program that is neither on `PATH` nor a program at the path given turns the line into "not saved: …" in the error color and disables Save, and `save_settings` refuses it as well; **Appearance** — the theme; and below them the actions row with the file, "Show file", Discard and Save. Save writes the file and locates satz again. There is no sign-in and no engine here: the app runs no model |
| Commands | `src/views/commands.rs` | not a destination: `PALETTE` is the table of every satz command the app runs, and `CommandDeck` renders any group of them — the list, the chosen entry's argument fields (a reporting command's format as a toggle group), the command line as it will run, Run and Cancel, and `CommandLog`, the streamed log with stdout and stderr distinguished, followed by the file a reporting command wrote where the app named it. `CommandPalette` is every entry in a dialog over the window, on ⌘K / Ctrl+K or the top bar's button, with `ONE_CLICK` — `whoami`, `transpile --check` and `questions`, palette entries run at their defaults — as one click each. Every entry opens in the format a person reads: `text` where satz offers it, `markdown` where it does not (`report-compliance`); `json` stays a choice in the format picker, and no entry fixes a format in its fixed words — `update-prerequisites` runs `--report-only` in satz's text. The log shows what satz printed, and the file a reporting command wrote, as satz wrote it. `APPLY` (`APPLY_DELEGATED` for a workload estate) and `COMPLIANCE` are the groups the destinations gather. For an estate that holds the preset library the palette's list ends with "Review a pack for the preset library" (`REVIEW_LABEL`, `CommandDeck { review }`), whose work area is the pack review card (`src/views/review.rs`, `PackReviewCard`, [ADR 0031](adr/0031-the-pack-review-is-in-the-palette-and-a-new-pack-opens-in-the-systems-editor.md)): "Choose a pack…" opens the OS file dialog on `.satz` files, in the folder of the last pack reviewed, else the estate's `presets_dir`, and runs `ReviewPack`; the switch "Judge it inside <estate>" adds `--against` the open estate, for a pack that estate uses. A review shows the pack's path, satz's verdict as a badge ("clears the bar", or "does not clear the bar yet" in the error color), the counts of errors, warnings and notes, "Review again" and "Close", the estate it was folded into and what the pack creates ("· creates …"); then the findings as a list — each with its line or "the whole pack" and the command satz names for it, clicking one sets the drawer's selection — beside the pack's text with line numbers, the selected finding's line marked and scrolled into view. Below, the two destinations as cards. **Upstream** says sending it is manual until satz ships contribute-pack and lists what the pull request to the satz repository takes: the file as `presets/<stem>.satz` or in the folder of its kind, a clean review (checked, or in the error color while an error stands), a changelog row, and no organization in the file — the review reads a private shape as an error, and each one becomes a param; "Copy the path" and "Open the folder". **Private** says the pack stays in this estate's library under the `.local.satz` name updates never touch, that a missing prerequisite row still goes upstream, names the file it goes to and offers "Place in the library" (`PlacePrivate`); a pack that is already the library's own file says so instead, and a name that cannot be a pack (`.diff.satz`) says why. A failed review is an error-colored row with the pack, satz's reason, "Review again" and "Close" |

A destination that works on an estate shows a card with a button to the Start screen
while none is open — which the sidebar cannot reach with no estate, but the banner can.

## 2. The component library

The window is built from the Dioxus component library,
<https://github.com/DioxusLabs/components>, and takes its look
([ADR 0034](adr/0034-the-window-is-built-from-the-dioxus-component-library.md)): styled
UI components over Dioxus's own unstyled, accessible primitives (`dioxus-primitives`).
The library documents its components at <https://dioxuslabs.github.io/components/>; what
follows is how this app uses them.

### The library's code (`src/components/`)

`dx components add <name>`, run in `crates/satz-studio`, copies a component's source and
its stylesheet into `src/components/<name>/` and the theme into
`assets/dx-components-theme.css`; `dx components update` replaces them. The copied code
stays as published and is never edited by hand; `src/main.rs` allows the variants and
sizes it brings and the app does not use. The UI components copied in are the ones the app
uses: `badge`, `button`, `card`, `collapsible`, `dialog`, `input`, `item`, `label`,
`progress`, `radio_group`, `sidebar` (with `separator`, `sheet` and `skeleton`, which it
is built from), `switch`, `tabs`, `toast`, `toggle_group` and `tooltip`.
`dioxus-primitives` is not on crates.io: the root `Cargo.toml` pins it to one commit of
the library's repository, and that pin moves together with `dx components update`. A new
control is a library component first; where a view needs something no component offers,
the answer is a part of the app's own built from library components (below), never a
changed copy of one.

The components' stylesheets are CSS modules that `dx` builds into the app, so the app is
run and bundled through `dx` (`dx serve`, `dx bundle`); a plain `cargo run` opens a window
without them.

### The look

- **Theme.** The library's, unchanged (`assets/dx-components-theme.css`): neutral greys,
  blue for focus, and red, amber and green for state, as CSS variables that each hold a
  light and a dark value. `Settings.theme` puts `data-theme="light"` or `"dark"` on the
  document root; `system` sets nothing, and the theme follows `prefers-color-scheme`.
  The window has no brand color.
- **Layout.** `assets/app.css` is the one stylesheet of the app's own. It arranges
  library components — the top bar over the view, the path bar and the drawer under it,
  the columns and rows inside a view — and gives the app's own elements (the tree, the
  connector tree, the command log, the banner, the monospace blocks) colors from the
  theme's variables. It never repaints or re-pads a library component: `dx` renames the
  components' classes when it builds, so a rule aimed at one would not match. The one
  rule that reaches into a library UI component is layout: a card's content (`data-slot="card-content"`)
  stands its children in a column with a 12 px gap, and a card that is one row — the
  Agent lead, the Apply state line, the map card, the empty states — puts its class on
  the content and lays that out as a row.
- **Icons.** Lucide line icons from `dioxus-icons` (`dioxus_icons::lucide`), one
  Dioxus component per icon, sized where it is used: `Check { size: 16 }`. Where an icon is
  chosen per entry it is chosen by the entry's id, in one function beside the table:
  `View::icon` and `Door::icon` (`src/state/mod.rs`), `Owed::icon` and the icon of a
  remedy's command (`src/views/overview.rs`), `CommandSpec::icon` (`src/views/commands.rs`,
  held to one icon per entry by `every_entry_has_an_icon_of_its_own`), and
  `AgentDocument::icon` (`src/views/documents.rs`).
- **Typeface.** Geist, the variable font, bundled as `assets/fonts/Geist-Variable.ttf`
  with its license beside it and declared in `src/app.rs` with `font-display: block`;
  the app works offline. Code and values are in the system's monospace face.

### Which library component each part is

| part of the window | library component |
|---|---|
| the destinations | `Sidebar` with `SidebarMenuButton`, `SidebarMenuBadge` for the counts, folding to icons (`SidebarCollapsible::Icon`) |
| every action | `Button`: `Primary` for the one a card or a dialog is for, `Secondary` beside it, `Ghost` for the quiet ones and the icon buttons (`ButtonSize::Icon`), `Outline` for Cancel of a running command, `Destructive` for Overwrite and Remove, `Link` for "and N more" |
| a count, a state, a key, an export's kind | `Badge` (`Outline`; `Destructive` for an error; `Secondary` for a count on a button) |
| every card | `Card`, `CardHeader`, `CardDescription`, `CardContent` |
| every list of rows to choose from — the drawer's findings, the palette's entries, a review's findings, the pack sources | `ItemGroup`, `Item` (`ItemSize::Sm`, `data-selected` on the chosen one), `ItemMedia`, `ItemContent`, `ItemTitle`, `ItemDescription`, `ItemActions` |
| a choice of a few — the theme, the capability ceiling, the Terraform tool, the kind of estate, the kind of source, a command's choice field, an export's format | `ToggleGroup`, `ToggleItem`, single-select by keeping one pressed |
| the folder a pack is stored in | `RadioGroup`, `RadioItem` |
| the Estate destination's three tabs | `Tabs`, `TabList`, `TabTrigger` |
| every dialog — the palette, the notice, a help text, a confirmation, a form | `Dialog`, `DialogTitle` |
| what flows through a pack, opened under its row | `Collapsible`, `CollapsibleTrigger` rendered as a `Ghost` `Button` with the in and out counts, `CollapsibleContent` |
| something running | `Progress` without a value |
| a field's label, a line of text | `Label`, `Input` (inside `Field`) |
| an on/off value | `Switch` (inside `SwitchRow`) |
| a tooltip | `Tooltip`, `TooltipTrigger`, `TooltipContent` (inside `Tip` and `SourceTags`) |
| what the stores report | the toasts of `ToastProvider` |

### The app's own parts (`src/widgets/`)

What the library has no UI component for, built from its parts:

| part | built from | what it is |
|---|---|---|
| `Field` | `Label`, `Input` | one labelled line of text, with one line under it that explains the field or, in the error color, says what is wrong with the value; `monospace` and `password` are its two variants; `onenter` and `onblur` are what a field commits on; with `label_hidden` — in a row that names its value at its start (`.param-row`, `.attr-row`) — the label is the input's accessible name and is not drawn |
| `SwitchRow` | `Switch`, `Label` | a switch with its label beside it, as one row |
| `TagList` | `Badge`, `Button`, `Field` | the values of a list, each a badge with a button that takes it out, and a field that adds one on Enter or blur, several with commas |
| `SourceTags` | `Badge`, `Tooltip` | a value as the estate file writes it: the literal text as code, and each `{param}` and each `${…}` reference as a badge whose tooltip says what it stands for — what a param resolves to, or that nothing binds it |
| `TypedField` | `SwitchRow`, `Field`, `TagList` | one field in the shape satz reads a value in (`FieldKind`: bool, number, list, text) over a `Draft`; a brace in a text and a non-number in a number field are refused under the field with satz's sentence, and `oncommit` fires only for a draft without a problem — on Enter, on a switch flip and a list change unless `commit_on_change` is off, and on blur unless `commit_on_blur` is off |
| `HelpButton` | `Button`, `Dialog` | a `Ghost` icon button with a question mark at the top-right corner of a card or a view, named "About <title>", opening a dialog that holds what the two lines of lead leave out, with one Close button |
| `Tip` | `Tooltip` | a tooltip around a control, its trigger a `span` so a button inside it is not nested in the library's trigger button |
| `Tree`, `TreeItem` | plain elements, Lucide chevrons | a nested list with a disclosure per branch; a row takes an `icon`, a supporting text and a `trailing` slot at its end |
| `ConnectorTree`, `ConnectorBranch` (`ConnectorLine::Solid`, `Dashed`; `error`, `trunk_error`, `label`) | plain elements | boxes joined by right-angle connectors: a root box, then per branch a vertical trunk from the parent and a horizontal branch into the child box, recursing through `branches`. The connectors are 2 px borders on the tree's own elements, the error variant in the theme's error color — nested lists and absolutely placed spans, no SVG and no measuring, so a tree cannot cross itself and follows every resize and zoom. The branch meets its box 32 px below the box's top. A `label` is one line of 20 px above the box. No disclosure: every branch is shown |

The banner (`SatzBanner`) and the door cards of the Start screen are the app's own
arrangements of library `Button`s and `Card`s, laid out in `assets/app.css`.

### Shell

- **Sidebar:** the library's `Sidebar`, beside the view, folding to icons: the top
  bar's first button folds and unfolds it, and a folded entry shows its name on hover.
  Unfolded it is 9.6rem wide, 60% of the library's 16rem: the library sets its widths
  inline on the wrapper, and `[data-slot="sidebar-wrapper"]` in `assets/app.css` overrides
  `--dx-sidebar-width` with `!important`, the one stylesheet rule that outranks an inline
  declaration. A name that does not fit is cut with an ellipsis, by the library's own rule
  on the entry's label; the folded width is the library's 3rem.
  The primary destinations in the order the work happens — Overview `LayoutDashboard`,
  Presets `Boxes` (an estate with the preset library only), Packs `Package`, Decisions
  `MessageCircleQuestionMark`, Apply `Rocket`, Security `Shield`, Documents `ScrollText`,
  Estate `FileText` —
  and, in the sidebar's footer, two secondary destinations, Agent `Bot` and Settings
  `Settings`. Overview carries the count of what
  the estate owes and Decisions the count of unanswered questions. With no estate open
  the primary group is empty and the footer carries Settings alone: there is nothing to
  work on, and the window stands on the Start screen. Opening an estate is what the Start
  screen does, and switching one is the top bar's action. The Interfaces view is a tab of
  Estate ([ADR 0023](adr/0023-the-interfaces-tab-reads-and-writes-through-the-cli.md));
  an estate without a preset library has no Presets destination
  ([ADR 0029](adr/0029-presets-and-packs-are-two-destinations.md)).
  `crates/satz-studio/src/state/mod.rs` holds `View::PRIMARY`, `View::SECONDARY` and
  `View::primary`, which leaves Presets out for such an estate. The library's sidebar
  turns into a sliding panel below 768 px of window width; the window's smallest width is
  900 px, so it stays beside the view.
- **Top bar:** a plain header of library buttons: the button that folds the sidebar,
  the estate's file name and directory, and beside them — not at the bar's far end —
  what acts on that estate: reload, "Switch estate", "Close estate", and a spinner while
  a reload runs. Then, at the far end, the estate's status and the drawer toggle,
  "Findings", with the count. The status is a `Ghost` `Button`: a check mark in the
  theme's success color while the estate owes nothing ("Nothing still to do"), a list icon
  with a `Badge` holding the count otherwise ("N items still to do") — the name is its
  tooltip and what a screen reader reads. It opens the Overview, where the list is;
  `owed_now` in `src/views/overview.rs` is the one derivation it, the sidebar's Overview
  count and the Overview's card read. On the Start screen it is not there. Close returns
  the window to the Start screen as it stands; Switch returns it there with the Open door
  showing, so the estates it has found are in front of you.
  Beyond that status the bar carries the estate's ADDRESS and nothing else: what the estate IS —
  the customer it stands for, whom its commands run as, its deployment mode, its schema,
  how far its HCL has been taken — is the Overview's identity card, so each fact has one
  place. Neither version is here: the window title carries satz-studio's, a newer release
  of either is the banner's business, and Settings holds the satz binary with its update
  buttons, the release page and the look-again.
- **Path bar:** while Overview, Presets, Packs, Decisions, Apply, Security or Documents shows, the
  window's foot — below the scrolling content and above the Findings drawer, so it never
  scrolls away — carries at its right edge a `Ghost` `Button` "← <step>" to the step
  before and a `Primary` `Button` "<step> →" to the next; each button's tooltip, opening
  upward, says what its step is for. `next_after` and `previous_before` in
  `src/views/path.rs` walk the same order both ways, and an estate without presets skips
  Presets in both. The first step has no previous button, the last no next. Estate,
  Agent, Settings and the Start screen have no bar.
- **Commands palette:** every entry of `PALETTE` in a dialog over the window, opened
  with ⌘K (Ctrl+K on Windows and Linux) — it has no entry in the sidebar — and closed with
  Escape, a click outside it or the same key. The listener is installed on the window in
  `src/app.rs`, which mounts once: a keydown inside a text field never reaches a handler
  above it, and a listener per estate would leave one behind on every switch.
- **Notice dialog:** what a pack asks to be run once it is switched on. A pack can name
  one command to run when it goes into an estate — `satz adopt` for the CIS org-policy
  packs, so every policy that is already live is in the state before the apply — and
  satz returns that notice in the report of the write that opened it (`satz_interview`,
  `satz_add_preset`, `satz_merge_presets`). The window holds them and raises a dialog over whatever
  destination the operator is on, one notice at a time, headed by the pack and carrying
  its sentence, the command as satz writes it, and what binding the notice's param
  means. Its actions are **Later** (lowers the dialog, the notice stands), **Run it**
  (the command in the app's log, over the Overview where that log stands beside the
  notice's own row, or in the OS terminal where the app runs that command)
  and **I ran it** (binds the param `true` through `satz_interview`, which is what an
  acknowledgement is). Run it is a delegated write: the main file is recorded before the
  command and checked after it, and put back when the check or satz refuses. A command the app will not run — one that is not satz's, one that
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
- **Findings drawer:** a bottom drawer, collapsed to its header, listing the open
  estate's findings, those of the pack the palette's pack review judged, and the errors
  the window reported ([ADR 0036](adr/0036-an-error-toast-is-a-count-and-findings-holds-the-text.md)),
  grouped Errors, Warnings, Info, each with `file:line` relative to the estate directory,
  its source (`satz review-preset` for a review's, `satz-studio` for the window's), and —
  for one of satz's findings — a badge naming the check that raised it
  (`unadopted-pack`, `missing-required`, `pack`, …). A row shows the first line of its
  text; clicking it sets the selection, shows the whole text under it when there is more
  than one line, and opens where its line is (`destination`): a line of the main file in
  Estate, a finding about the reviewed pack in the palette, opened on its pack review
  (`Go::Review`). The window's errors are kept in `AppStore::errors`, newest first in the
  list and at most `ERRORS_KEPT` (50), with an estate open or none; closing an estate and
  opening one empties the list (`close_estate`), so an estate reopened after an edit shows
  what satz says of it now. An error that is a finding of the estate already — what a
  reload's check found, a write satz refused or rolled back — raises its toast through
  `toast_finding` and is not kept: the next reload is what updates the finding. One whose
  text a finding carries is listed once (`reported`). While there are any, "Clear the
  window's errors" (ghost, `X`) empties that list.
- **Toasts:** what the stores report is a toast of the component library (`ToastHost`
  inside the `ToastProvider` of `src/app.rs`). A notice is its own text and leaves by
  itself after five seconds. An error is not quoted: the errors that arrive together are
  one toast titled with their count, "1 error" or "3 errors" (`error_title`), whose line
  says the Findings drawer holds the full text, and it leaves after eight seconds; `toast`
  keeps each error's text in `AppStore::errors` for the drawer, and `toast_finding`, for an
  error that is a finding already, keeps none. Either can be closed before.

### The agent handoff

satz-studio runs no model
([ADR 0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
The Agent destination gives an external client the estate on screen, and Settings names
that client; there is no sign-in and no engine anywhere in the window.

- **What the cards show** is what satz printed. Each card runs `satz mcp-config
  <estate> --client <client> --allow <ceiling>` when the view opens and again when the
  estate or the ceiling changes, and renders its stdout as the block and its stderr as
  the notes under it. The binary and the root in that block are satz's own; the window
  states neither. The ceiling is the one Settings holds for the agent, and the lead card
  carries it as "Settings: <ceiling>"; it bounds the satz server only, since an agent
  with a shell can run satz commands directly
  ([ADR 0021](adr/0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md)).
- **What the client runs at** is read from its file after every run of the card: the
  file satz's notes name, and satz's key in it, against the block on screen. "not
  configured" when the file or the key is not there; "configured: <ceiling>" when the
  key holds that block; and, when it holds another entry — the ceiling Settings held
  when it was written, another binary, another root — "configured: <ceiling on disk>" in
  a block in the theme's warning color naming the file and both ceilings, with "Replace
  it", the `--write --force` run. A file satz's key cannot be read from is said in the
  theme's error color. Saving Settings rewrites no client file; the file changes when Configure or
  Replace is pressed.
- **Configuring a client.** "Configure Claude Code" and "Configure Claude Desktop" run
  the same command with `--write`, which merges satz's key into the file that client
  reads and leaves every other server in it alone. The toast is the line satz ended on —
  written, added, replaced or unchanged — and the card holds all of it. A refusal stands
  in the card in the theme's error color, word for word as satz wrote it; where satz's own
  refusal names `--force`, "Replace it" under it runs `--write --force` — offered once,
  by the on-disk state when that already shows another entry — and nothing else in the
  window passes that flag.
- **Copying.** "Copy" puts the block satz printed on the clipboard, for a client
  configured by hand.
- **Starting the client.** "Open in <client>" runs the configured command in the estate's
  directory, in the OS terminal, the way `apply` and `bootstrap` are run. The button is
  disabled and reads "No agent configured" while Settings name none; a command that is
  not installed is an error toast naming it. There is no terminal in the window, nothing
  is supervised, and the estate is re-read when the window comes back to the front.

How the cards are laid out: the lead is a `Card` whose content is one row (`.agent__lead`
in `assets/app.css`) — an icon, the text and a `Badge` with the ceiling; each client card
holds the block satz printed and its notes as monospace blocks on the theme's surface
(`.agent__config`, `.agent__notes`), what the client's file holds as a `Badge` beside a
line naming the file (`.agent__state`, `.agent__differs`, the latter with "Replace it"
under it), and a refusal as a block in the theme's error color inside the card that raised
it (`.agent__refusal`), with the run that answers it.

## The smoke walk

The manual check of the estate views, over a copy of satz's own smoke estate — a
scratch folder holding `vendor/satz/tests/smoke/yaml/*.satz` under `yaml/` and a
`config.toml` like `tests/fixtures/smoke/config.toml` with `yaml_dir = "yaml"` and the
other paths absolute into `vendor/satz`, because `satz mcp` opens only an estate file
inside the directory holding its `config.toml` — and over a skeleton written by
`satz interview <dir>/yaml/new.satz --create`, which is the estate every pack line
starts commented in. Every write is checked by `satz transpile --check` through the
estate's `satz mcp` child. No step needs a sign-in of any kind; the first runs
`satz init`, which reads the sign-in of whoever is signed in where there is one, and
step 12 needs an agentic client installed. Nothing in the walk changes a live
organization: `bootstrap` and `apply` are read as command lines, never run.

0. **Create.** Estates → Create → a folder and a customer id → Create. The window lands
   on **Decisions**, not on Overview: the skeleton has 18 questions of its own and nothing
   else yet. Overview's identity card opens with the short name, the customer, the
   customer id and the organization id as the skeleton's `params { }` block has them —
   the customer id given, a param `satz init` wrote as `""` reading "empty" — and carries
   the rest of those questions as rows below, each reading "not answered"; the badge in
   its header counts every row without a value.
1. **Import.** Import → an empty folder → Terraform HCL → a directory
   holding a `.tf` file. The card under the folder says satz init runs first; the
   preview shows both command lines. Import → the log carries both runs, the report
   card names the file satz wrote and every block it promoted or wrapped, and that
   estate is open in the top bar. Choose "State document" and point it at a raw
   `.tfstate`: the field turns red with satz's own sentence and Import stays disabled.
   On "State document" and "Terraform HCL" the Organization field is there, and the
   preview carries `--organization <n>` once it holds a value; on "Live scope" the field
   is gone.
2. **Open.** Open → the folder → Open. The window lands on Overview; the top bar shows
   the file and its directory, with reload, switch and close beside them. The first card
   on the view is the identity card: whose estate it is, by short name, customer,
   customer id and organization id, read from the file's `params { }` block, then the
   domain, the infrastructure it names and the service account it runs as, each one
   the estate's own answer and an unanswered one saying so; below them whom the live
   calls run as, the deployment mode, the schema with its provider and type count, and
   what the HCL directory holds.
   The sidebar carries the owed count on Overview and the unanswered count on Decisions.
3. **Read what is left.** The Overview's "Still to do" card lists a row per thing and
   nothing else. An estate made in a folder outside any repository carries "The estate
   is not in a git repository" first; "Create the repository" streams `git init -b
   main`, `git add -A` and the commit into the log and the row goes (with no git
   identity configured, the log ends in git's own refusal and the row stays). Answer a question (step 4) and the questions row loses one; answer the
   last and the row goes. Point the estate at an empty `schema_dir` (step 9) and the
   schema row appears with "Run update-schema" on it. An estate that owes nothing shows
   no card of rows, and the top bar's status turns to the green check mark, "Nothing still
   to do"; with something owed it carries the count, and pressing it opens the Overview.
4. **Answer a question.** Decisions → the first open question: the primary button reads
   Accept and Skip stands beside it. Type into the field: nothing is written, and the
   button still reads Accept. A list question that offers nothing — the access-approval
   pack's `access_approval_notification_emails` on an estate that uses it — is a tag
   list with "no default — at least one value is needed" and Accept disabled; one address
   and Enter adds the badge and writes nothing, then Accept writes
   `access_approval_notification_emails = ["…"]`, a list of one. Accept: the toast says "1 answer written"; the file has one new line in
   `params { }` (`git diff` shows nothing else: no rewritten line, comments and alignment
   intact); the question count on the sidebar drops by one; a finding the transpile had
   raised for that param is gone from the drawer; the card is on the question that
   followed, and the answered one has left the list. Press Back: the switch goes on and
   the card returns to the question just answered, its badge reading "answered: …", its
   primary button reading Next and no Skip. Type into the field: the button reads Accept;
   restore the value and it reads Next again. Press Next: the card moves on and nothing
   is written. Shrink the window's height until the question does not fit: the question
   scrolls and the bar with Back, Skip and the primary button stays below it.
5. **Switch the map on, then a pack.** On the skeleton: Presets → "Switch the map on" →
   the line `use "presets/estate-map.satz"` is uncommented and the map's questions are
   open. Switch the budget pack on → `use_budget = true` lands and satz uncomments
   `use "presets/organization-budget.satz" when use_budget`; switch it off →
   `use_budget = false` and the line stays active, the badge reading "active" without
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
   shows the check's finding at that line, source "check", and it stays after the
   reload; clicking it in the drawer opens Estate → Resources with the node at that line
   selected; the file is byte-identical to before.
9. **Missing schema.** Point a copy's `schema_dir` at an empty directory and open it:
   the Overview carries the schema row and its "Run update-schema", the identity card
   says "none in <dir>", and Estate → Resources locks every row with "no schema".
10. **The palette and the two halves of the bootstrap.** ⌘K opens the commands palette over
    whatever destination is showing; Escape closes it. Apply → `update-prerequisites`
    runs `--report-only` and prints the gap; where the transpile found one, the card above
    the deck offers "Write them into the estate" and the toast counts the lines written.
    Apply → `apply`, `migrate` and `bootstrap` offer "Copy" and "Open in terminal" and
    no Run; `bootstrap --dry-run` runs here. **Do not run a real
    `bootstrap`** against an organization you are not prepared to change.
11. **Leave the estate, two ways.** The top bar's "Switch estate" returns the window to
    the Start screen with the Open door showing and the estates it found listed; "Close
    estate" returns it there as it stands. Either way the sidebar carries Settings alone.
12. **Set an agent up on the estate.** Agent → the lead card says the app runs no model
    and that the ceiling bounds the satz server only, with "Settings: <ceiling>" as a
    badge, and each client card shows the block satz printed with its notes under it; the
    Claude Code card reads "not configured". "Configure Claude Code" → the toast is satz's
    line, `.mcp.json` is beside the estate's `config.toml`, its `mcpServers.satz.args`
    holding the estate's directory and that ceiling, and the card reads "configured:
    <ceiling>". Press it again → "unchanged" and the file's mtime does not move. Lower the
    ceiling in Settings → Save: `.mcp.json` is unchanged. Agent → the Claude Code card
    reads "configured: <the old ceiling>" in the block in the warning color, naming the file and
    both ceilings, with "Replace it"; "Replace it" writes the new ceiling into the file
    and the card reads "configured: <new ceiling>". With the ceiling at `read`, answer a
    question in Decisions: it is written, since the window's own `satz mcp` runs at
    `read,write`. Put `{` in the file and press
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
    script that answers `--version` with a version one patch past the build's satz (the
    version `vendor/satz/Cargo.toml` names, X.Y.Z below) and hands everything else to the
    installed satz — `#!/bin/sh`, then `if [ "$1" = --version ]; then echo 'satz
    X.Y.Z+1'; else exec ~/.local/bin/satz "$@"; fi` — make it executable, and set it as
    the satz binary in Settings → Save. The tertiary banner names that version and the
    build's satz and says it is a patch release, with the satz-studio look's sentence under
    it; the Open door opens the smoke estate's copy. "Dismiss" → the banner goes and
    `settings.toml` reads `dismissed_satz = "X.Y.Z+1"`. A second script like it answering
    `satz X.Y+1.0`, set as the satz binary → Save: the notice is back and says it is a
    minor release. Clear the path → Save, and the installed satz is in use again.
14. **Export the decisions.** Documents → the "Decisions sheet" card offers the
    formats `satz questions --help` lists (`text`, `markdown`, `pdf`, `json`, `xlsx` at
    the pinned satz), on `markdown` with nothing under it, on `xlsx` with satz's line "a
    workbook: the catalog a customer fills in and sends back". "Export…" → the save
    dialog opens in the estate's directory on `<estate>-decisions.md` → Save: the log
    shows the `satz questions … --format markdown --out …` line, the toast names the
    file, and it opens in the application the system gives Markdown. Pick `xlsx` →
    Export… → Save: the workbook opens in the spreadsheet application. Answer a
    question, then "Export again": the same file is written over and opens with the
    answer in it. Documents offers the same, and "Export again"
    there writes the last export's file.
15. **Review a pack, then place it.** Copy
    `crates/satz-studio-core/tests/fixtures/review/team-access.satz` to a scratch folder
    and open the estate step 0 created, whose library is its own. ⌘K → "Review a pack for the preset library" → "Choose a pack…" →
    the copy. The toast reads "the pack does not clear the bar yet" with its errors
    counted, and the drawer opens with them — not formatted and the membership among them
    — and the notes, each from `satz review-preset`. Close the palette and click the membership in
    the drawer: the palette opens on the review with the membership's line marked in the pack's text. The Upstream card
    says sending it is manual until satz ships contribute-pack, the clean-review
    line in the error color; "Copy the path" puts the path on the clipboard, "Open
    the folder" opens the scratch folder. The Private card names
    `<presets_dir>/team-access.local.satz`; "Place in the library" → the toast names
    the file, the review now names the placed file, and the Private card says it is the
    library's own. Edit the scratch copy, review it, and place it again: refused, the
    fork in the library holds other text. "Close": the card's review and the drawer's
    review findings go. Review `vendor/satz/presets/organization-budget.satz`: "the
    pack clears the bar", the drawer stays closed.
16. **Read the interfaces.** Open the copy's `showcase.satz` → Estate →
    "Interfaces (4)": the core card lists `infra_folder` and the other core exports,
    `folders` and `buckets` with a `map · <type>` badge; `audit` carries the `common` badge;
    `archive` reads "uses audit" and its `archive_project_id` an `attach
    google_project_iam_member` badge; `orders-events` and `billing-events`, which the
    showcase's top-level `each` writes, carry no `common` badge, read "uses audit" and
    export `topic`. Last, the card "Lists that take entries" carries the list param `event_topics`
    with the badge `key name`, its entry count, `showcase.satz:<line>`, a secondary "Add",
    the fields `name ~ [a-z][a-z0-9-]*, retention ~ [0-9]+s` and the description the
    showcase gives it.
17. **A workload estate's needed roles.** Copy `crates/satz-studio-core/tests/fixtures/workload/`
    beside satz's `tests/corpus/project/interface.satz`, its `config.toml`'s paths made
    absolute, and open `workload.satz`. Overview → the Kind row reads "Workload estate";
    the Prerequisites row reads "Roles are the organization's to grant" and "Needed
    roles". "Needed roles" → the dialog shows a progress bar, then the estate account,
    "On the project" with `roles/storage.admin` and the others, "On the folder" with
    `roles/resourcemanager.folderAdmin or roles/resourcemanager.folderCreator` for
    `google_folder`, and "APIs on the delegation's infrastructure project" with
    `storage.googleapis.com on acme-infra-001`. "Copy" → the toast reads "Copied", and the
    clipboard holds the account's line and one line per role and per API. "Still to do"
    has no item about roles; Apply's state line reads "APIs declared".
18. **Scan the estate's own resources.** In the same copy, before a transpile: Security →
    the card says nothing is transpiled yet and "Scan" is not offered. Apply → run
    `transpile`, then Security → "Scan": a progress bar, then the card counts the scan,
    and each resource with a failed check is a row with "declared at workload.satz:<line>"
    and its checks, each with Checkov's id and words. "Guideline" opens Checkov's page.
    With no `checkov` on `PATH` and no `uvx`, the card shows satz's refusal naming both.
