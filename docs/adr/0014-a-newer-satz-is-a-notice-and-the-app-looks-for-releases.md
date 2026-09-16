# 0014 — a newer satz is a notice, and the app looks for releases of itself and of satz

- **Status:** accepted
- **Date:** 2026-09-16
- **Deciders:** the maintainer

## Context

The app is built and tested against one satz, the release the submodule `vendor/satz` is
pinned to ([ADR 0002](0002-a-separate-repository-with-satz-pinned-once.md)). A satz older
than `MIN_SATZ` is refused, because the JSON shapes and tool names the app reads are that
release's. A satz at or above it was accepted without a word.

satz releases several times a day. Most operators who run `satz self-update` are on a satz
newer than the app's build, so an app one release behind satz is the usual case, not a
rare one. satz's own release rule says how much the gap matters (satz ADR 0010,
`vendor/satz/docs/adr/`): a MINOR release is one after which the same estate or input
needs an edit, is refused, or plans differently; a PATCH changes nothing an estate needs.

Three facts shape the answer:

- **CI installs the NEWEST satz release on purpose.** `scripts/install-satz.sh` follows
  `releases/latest`. A red run after a satz release is the alarm that the app has fallen
  behind, and it is answered with a satz-studio patch release the same day: the pin moves,
  then the version and the tag. On any day after a satz release, every test that locates
  the real binary sees a satz newer than the one the app was built against.
- **The app does not update itself.** It is unsigned, and an unsigned app replacing itself
  trips Gatekeeper and SmartScreen. Pointing at a newer release is as far as it goes. satz
  does update itself (`satz self-update`, which verifies its installer's SHA-256).
- **An operator only learns about a release they are told about.** An on-demand look is
  a button nobody presses at the moment it matters.

## Decision

**The app copes with a newer satz and tells the operator; the only run-time refusal is a
satz below `MIN_SATZ`.**

- The driver locates a newer satz and refuses nothing about it.
  `SatzBinary::ahead_of_build` says whether it is newer and by which kind of release
  (`Ahead::Patch`, or `Ahead::Minor` — a moved major reads as a minor), comparing the three
  numbers with the submodule's satz (`SatzBinary::built_against`, read from
  `vendor/satz/Cargo.toml` as the crate compiles). No release notes are fetched.
- A newer satz is `SatzStatus::Located`, and every estate opens. A notice says "satz X is
  installed; this satz-studio was built and tested against satz Y", followed by satz's
  reading of the difference. **Dismiss** hides the notice for that satz version only
  (`Settings.dismissed_satz`). It grants nothing and refuses nothing.
- **Once per launch, never on a timer, the app looks for releases on its own**, and keeps
  the result for the session:
  - **satz-studio:** the latest release of `tjirsch/satz-studio`, read from GitHub's API and
    compared with the build's version. Nothing is downloaded, run or written.
  - **satz:** `satz self-update --check-only` on the satz in use. satz owns its updater, so
    the app runs satz's check rather than reading GitHub for satz a second time. Only the
    `Latest version:` line is read, compared with the version of the binary that printed
    it. The run is skipped when the operator's satz config says
    `self_update_frequency = "never"`: satz may not look for releases unprompted, and the
    app does not look on satz's behalf either. "Check only" asks satz when the operator
    wants.
- **`MIN_SATZ` is the oldest satz this build works with, and it rises for a reason, not
  with the pin.** It may sit below the submodule's satz; a test holds it at or below. It
  rises only when (1) a satz release breaks the app, which a satz-studio patch release
  answers the same day, or (2) the app starts using something a later satz introduced — a
  flag, a tool, a report field — whose tests are what make that version the requirement. A
  routine pin bump moves the submodule and the recorded reports and leaves `MIN_SATZ`
  alone. Its value stays where it was when this was decided (0.59.7): nothing established
  which older satz the app still works with, and lowering it on a guess would be the
  untested claim this rule avoids.
- **Where it says so.** The window title is `satz-studio <version>`, followed by
  "update available" for either release that is newer. A title bar cannot be clicked, so
  the top bar carries the same two facts as chips that act: the satz-studio chip opens the
  release page, and the satz chip runs `satz self-update`. A look that fails, whether
  GitHub is unreachable or the unauthenticated API returns its 403 for the hourly limit,
  puts its reason in Settings and raises no toast.

## Consequences

- An operator who updates satz keeps working; the only thing that stops the app at run time
  is a satz below the minimum.
- A satz release that breaks the app shows as a failure where the app reads satz's output —
  a deserialisation that fails, a tool the session does not list — rather than as a refusal
  up front. The answer is a satz-studio patch release, and the launch look brings it to the
  operator.
- Two GitHub API requests per launch: satz's check and the app's own look. Both count
  against the sixty requests an hour the unauthenticated address is allowed, a limit shared
  with every other `satz` command that checks.
- The notice is tested with fake binaries through the driver and the app's `satz_notice`:
  the pin gives no notice; a newer patch and a newer minor give one each and still open
  estates; a dismissed version gives none, and the next version gives one again.
- The tests that locate the installed satz assert `MIN_SATZ` or newer and nothing tighter,
  so they pass on CI's newest satz unchanged.
- **The minimum is not run against itself.** satz keeps only its five newest releases, so CI
  cannot install the satz `MIN_SATZ` names and test the app against it. The minimum rests on
  (1) and (2) — a breakage seen, a use introduced — rather than on a run against that
  version. This cost is accepted.
- A routine pin bump no longer asks every operator to update satz: an operator on
  yesterday's satz keeps working until a breakage or a use moves the minimum.

## Pros and cons of the options

### A — accept any satz at or above `MIN_SATZ` without a word

- **Good:** nothing to show, nothing to remember.
- **Bad:** the operator cannot tell a satz the app was tested against from one it was not,
  and learns of a satz-studio release only by looking for one.

### B — refuse a newer satz in the driver, as an older one is refused

- **Good:** one rule; the app never runs a pairing it was not tested with.
- **Bad:** CI installs the newest satz, so every test that locates the real binary goes red
  on the day satz releases, for the wrong reason. An operator who updated satz cannot work
  until they downgrade it or satz-studio catches up.

### C — let a newer satz run only after the operator chooses, once per satz release

- **Good:** the operator sees the gap before anything runs; the choice expires with the
  release it was made for.
- **Bad:** the usual case is blocked. satz releases several times a day, so the choice
  comes several times a day, and it asks a question with only one practical answer: run it.
  This was built on 2026-09-16 and replaced the same day.

### D — a notice, with the app looking for releases of itself and of satz at launch *(chosen)*

- **Good:** nothing is blocked. The operator is told which satz runs, what satz's rule says
  about the gap, and whether a satz-studio or satz release is available, without having to
  ask. CI keeps its alarm.
- **Bad:** two network requests per launch. A breakage caused by a newer satz is met where
  it occurs, not before. A dismissed notice is one more settings field.

### For the minimum: E — `MIN_SATZ` held equal to the submodule's satz

- **Good:** every satz the app accepts is one it was tested against; one number to move.
- **Bad:** every pin bump — several a day, when satz releases — refuses the satz an operator
  had an hour ago, although nothing broke. The refusal says "update" and means nothing.

### For the minimum: F — the oldest satz the app works with, raised for a breakage or a use *(chosen)*

- **Good:** an operator is refused only for a reason the app has — a satz that breaks it, or
  one lacking what it uses — and the reason is in the change that raised it.
- **Bad:** the minimum is not run against itself, because satz keeps five releases; it is
  as good as the attention paid to (1) and (2).
