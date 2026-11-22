# 0035 — Security is a destination of hints on the estate's own resources

- **Status:** accepted
- **Date:** 2026-11-22
- **Deciders:** the maintainer

## Context

A person who deploys resources through an estate, or writes the pack that declares them,
learns that one of them breaks a security control late: in the security review, which
an agent runs with satz's `security-review` skill over the whole organization, or not at
all. A short hint at the moment of writing — "this bucket logs no access; change it or be
ready to say why" — is missing.

satz has three ways to judge an estate's security, and they differ in what they need:

- `satz require` and `satz report-compliance` judge the estate against a catalog of
  controls in the preset library's `catalogs/` folder. A workload estate has no preset
  library, so both refuse it ("No catalog … in presets/catalogs"), and building more
  catalogs into satz is not wanted.
- `satz prowler` prints the Prowler command for the estate, and Prowler reads the live
  cloud. For a workload estate the command it prints today covers the whole organization,
  which a team working in a delegated folder must not scan and is not allowed to.
- `satz scan` runs Checkov over the HCL code the last transpile wrote and points each
  failed check at the Satz line that declared its resource. It reads the estate's own
  code only: no sign-in, no catalog, nothing of the organization. Checkov's open-source
  edition names no CIS control, so a finding carries the check's own words.

The app reads satz's reports as data and judges nothing itself (ADR 0025, ADR 0032).
`satz scan` printed text only until satz v0.110.1 added `--format json`.

## Options

**Where the hints stand**

1. **A destination of its own, between Apply and Documents.** The scan reads what Apply's
   transpile wrote, and the written reports come after it. Costs one more sidebar entry
   and a step on the path bar.
2. **A card on Documents.** No new destination; Documents mixes a judgement with the
   files handed to people, and the page grows long.
3. **A card on Apply.** Apply is the way to the organization (ADR 0028); a hint about a
   pack belongs before the plan, but Apply already holds the sign-in, the state and nine
   commands.

**What the hints are read from**

1. **`satz scan`, Checkov over the estate's code.** Works for both kinds of estate today,
   offline. No control numbers.
2. **Prowler, through the command `satz prowler` prints.** Control numbers for Google
   Cloud, AWS and Azure from Prowler's own findings, after a deploy only. Needs satz to
   print a command limited to a workload estate's own projects.
3. **`satz report-compliance` filtered to the estate's resources.** Needs a catalog, which
   a workload estate does not have.

**A justification for an accepted finding**

1. **Not in this step.** The person writes it; satz has `claim … deviates { reason = … }`
   for a control its catalog lists, and whether that serves a check outside a catalog is
   open.
2. **A field per finding that writes the reason.** Needs a satz write and a decision
   about where the reason lives.

## Decision

Security is a primary destination between Apply and Documents and a step of the path.
It reads `satz scan <estate> --format json` when "Scan" is pressed — Checkov runs for a
while and is never run unasked — and lists the failed checks by resource: the resource,
the Satz file and line that declared it, each check's id and words, and Checkov's
guideline. Before anything is transpiled, "Scan" is not offered and the card says the
transpile on Apply writes what the scan reads. A refusal — no Checkov, no HCL code — is
satz's words in the card. The last scan is held for the open estate until it is scanned
again or the estate closes; nothing is written.

Prowler is the next step and waits for satz: a command limited to a workload estate's own
projects, which the app then starts, and failures shown with their control ids. A
justification is not part of either; the person writes it, and it is left for later.

`MIN_SATZ` is 0.110.1, the first satz whose `scan` prints JSON.

## Consequences

- A workload estate gets the same hints as an organization's, from its own code, with
  no sign-in.
- The hints name no control number. Where a number matters, the security review on
  Documents and, later, the Prowler step give it.
- The scan says nothing about an edit that is not transpiled yet: it reads the files
  the last transpile wrote.
- The sidebar holds eight primary destinations, seven of them steps.
