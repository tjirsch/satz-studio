# 0006 — `apply` and `bootstrap` run in the user's terminal

- **Status:** accepted
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

Two satz commands change a live organisation and are built around a human at a
terminal. `satz apply` is a thin wrapper over the configured tool in the estate's
`hcl_dir`: stdio is inherited, so `tofu apply`'s approval prompt — the step that shows
the plan and waits for `yes` — behaves as it does from a shell, and the tool's exit
code is the command's. `satz bootstrap` creates the day-0 infrastructure as the
human's own identity after an interactive pre-flight, then runs `init` and the first
imports through the same wrapper. satz's MCP server serves neither (`MCP_PARITY` in
`vendor/satz/src/mcp.rs`: `apply` "hands stdio to the tool, approval prompt included";
`bootstrap` runs "as the human, after an interactive pre-flight"), and under MCP stdin
and stdout are the protocol.

The app runs every other command itself: `SatzCli` streams stdout and stderr into the
Commands pane, and `plan` runs there without a terminal: satz writes every variable's
value into `terraform.tfvars`, so tofu asks nothing. The question
is what the app does with the two commands whose interaction is the safety step.

## Decision

They run in the user's own terminal. `EstateSession::external_command`
(`crates/satz-studio-core/src/satz/session.rs`) writes a one-shot script under the
app's data directory — `<data dir>/satz-studio/run/<unix millis>.sh`, `.cmd` on
Windows — holding `cd "<estate>" && "<satz>" --config . <args…>`, quoted for the
platform's shell and made executable. `open_in_terminal` opens it with the OS
terminal: macOS `open -a Terminal`; Linux the first of `x-terminal-emulator`,
`gnome-terminal`, `konsole`, `xterm` found on `PATH`, running `bash <script>`; Windows
`cmd /c start "satz" cmd /k <script>`. The app shows that the command is running in
the terminal and reloads the estate when its window regains focus.

`-auto-approve` is never passed. Nothing in the app can answer the tool's prompt.

## Consequences

- A context switch for the two commands that change a live organisation: the operator
  reads the plan and types `yes` in a terminal window, not in the app.
- The app learns nothing from the terminal. How the window closed says nothing about
  the command it ran; what the app knows is the estate as it is on the next reload,
  and the tool's own state in `hcl_dir`.
- A Linux desktop with none of the four terminals on `PATH` gets `SatzError::NotFound`
  naming them; there is no fallback to running the command inside the app.
- The script names the estate's directory and the satz binary in clear, under the
  app's data directory, and stays there after the run.
- The seam is one method. An embedded terminal (a PTY and a terminal widget) is an
  option behind `external_command` for a later version, without changing what `apply`
  means.

## Pros and cons of the options

### A — a one-shot script in the OS terminal *(chosen)*

- **Good:** the tool's own prompt is the approval, unchanged; the app carries no PTY,
  no terminal emulator and no way to say `yes`; the operator's shell, colours and
  scrollback are theirs.
- **Bad:** a second window; the app cannot show the outcome, only the estate after.

### B — an embedded terminal emulator

- **Good:** one window; the plan and the prompt beside the estate they change.
- **Bad:** a PTY per platform and a terminal widget to maintain, in a webview, for two
  commands; and an app that can render the prompt can be made to answer it. An option
  for a later version behind the same seam.

### C — `apply -auto-approve` after the app's own confirmation

- **Good:** the whole flow stays in the app, with a Material dialog for the plan.
- **Bad:** it moves the safety step out of the tool that owns it. The prompt satz
  inherits is the tool showing what it is about to do, at the moment it does it; a
  dialog in the app shows a plan the app ran earlier, and an agent's turn is one
  approval card away from an apply.

### D — not offering these commands

- **Good:** the app changes no live organisation and needs nothing above `read,write`.
- **Bad:** an operator who has answered the interview, adopted the ids and read the
  plan in the app leaves it to type the command by hand, at the moment the app has the
  most to show.
