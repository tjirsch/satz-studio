# 0028 — Apply is one destination, and it starts with who is signed in

- **Status:** accepted
- **Date:** 2026-11-18
- **Deciders:** the maintainer

## Context

The rail had two destinations for the last part of the work: Checks, a list of five
commands that judge the estate, and Deploy, a list of six that hand it off. Each was a
menu, grouped by kind of command. Neither said where the estate stands or what comes
next, and a person walking an organisation from an empty estate to an applied one had
to know that the day-0 dry run is "a check" and day 0 itself "a deploy".

Every step that reaches Google needs credentials on the machine. Nothing in the window
said whether there are any, or whose: a command was run and failed, and its log was
where the missing sign-in showed.

satz answers that question as data. `satz_whoami` reports the credentials, the account
the open estate's live commands run as, and — online — whether these credentials may
act as that account, whether the project the calls are billed to is reachable, and
whether the account holds the permissions the estate's resource types need. A
credential that mints no token is its refusal, in words that name the sign-in command.

The path wanted is one line through the window: open, choose the components, answer
their questions, apply, hand over the documents.

## Options

**The last part of the path**

1. Keep Checks and Deploy. Cost: two menus for one walk, and no place for its state.
2. **One destination, Apply**, with the commands in the order they are walked.
3. A wizard that leads from step to step. Cost: an estate is revisited far more often
   than it is first rolled out, and a wizard is in the way every time after the first.

**The sign-in check**

1. At every open. Cost: a network call the operator did not ask for; the window makes
   none unasked (ADR 0024).
2. **When Apply is entered, once per estate, and on a button.**
3. Before each command that calls Google. Cost: the app does not know which do; satz
   keeps that table.

**The compliance views** (`require`, `report-compliance`), which stood in Checks

1. In Apply. Cost: neither is a step toward an applied estate.
2. **In Documents**: their result is a report.

**The documents an agent writes** (satz's skills for the estate's documentation and its
security review)

1. Run them in the window. Cost: the app runs no model (ADR 0020), and a window around
   an agent is a poor copy of the agent's own.
2. **Start the configured client on the skill**, in the estate's directory.
3. Leave them out. Cost: the path ends before its last step.

## Decision

- **Apply replaces Checks and Deploy.** It shows, from the top: the sign-in card; a
  state line — whether the estate compiles, whether its rights are declared, whether the
  generated code is written and its directory initialised; the card that writes the
  prerequisites, where the compile found any missing; and one deck of commands in the
  order they are walked — `transpile --check`, `update-prerequisites`, `bootstrap
  --dry-run`, `bootstrap`, `transpile`, `hcl-init`, `plan`, `apply`, `migrate`.
- **The sign-in check is `satz_whoami`, online**, sent when Apply is entered and no
  answer is held for the estate, and by "Check again". Its report is shown as satz gives
  it; its refusal is shown in satz's words with the sign-in command. While the last
  check is a refusal, every command deck says so above its command line.
- **The compliance commands are a deck of Documents.**
- **The agent's documents are cards of Documents**: each starts the client Settings
  names with the skill as its opening instruction (`<client> "/<skill>"`), in the OS
  terminal, as the Agent destination starts it. The app does not check that the skill is
  installed, and reads nothing back.
- The rail holds six primary destinations: Overview, Presets, Decisions, Estate, Apply,
  Documents. One of Material 3's seven is free again
  ([ADR 0027](0027-documents-is-a-destination-of-its-own.md)).

## Consequences

- The window does not know that an apply happened: `apply` runs in the operator's
  terminal (ADR 0006), and only a plan read afterwards says what is left.
- The warning on the command decks is on every command, including the ones that call
  nothing, because the app does not know which do. The exact gate is satz's to give.
- A command that calls Google is still offered when nobody is signed in. It fails as it
  did; what changed is that the reason is on the page before it is run.
- The skill's command line is Claude Code's form. Another client is started with an
  argument it may not read.
- Apply offers an organisation's commands to every estate; an estate in a delegated
  place has no day 0. That follows with the workload path.
