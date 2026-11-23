# 0036 — An error toast is a count, and Findings holds the text

- **Status:** accepted
- **Date:** 2026-11-23
- **Deciders:** the maintainer

## Context

The window reports a failure as an error toast, and about seventy places hand it the
whole message: satz's stderr, a refusal over MCP, an installer's last lines. A toast is a
small box in the window's corner; a satz message of several lines does not fit it, and it
leaves after twelve seconds whether it was read or not.

Some of those errors are also findings in the Findings drawer — a write satz refused is
both — and the rest are not: a setting not saved, a pack source refused, an installer
that failed, a page that would not open. The drawer showed only the first line of a
finding, so a long message was readable in full nowhere but in the toast.

The component library's toast carries a title and a line of text and no button, so a
toast cannot open a dialog without editing the library, which the window never does
(ADR 0034).

## Options

1. **A count in the toast, the text in Findings.** Every error the window reports is kept
   in full and listed in the drawer; the toast says how many came. The drawer becomes the
   one place errors are read, whether an estate is open or not. Costs a list the window
   keeps and a way to clear it.
2. **The first line in the toast, cut to fit.** Small and says what failed; the rest of a
   message that is not a finding is lost.
3. **A dialog listing the errors after an action.** Full text where it happened; one more
   window to close after every failure, and the actions that report an error are many and
   unrelated.

## Decision

Option 1. `toast` keeps each error's text in `AppStore::errors` (at most 50, the oldest
going first) as a finding whose source is the window. `ToastHost` shows the errors that
arrive together as one toast titled with their count, "1 error" or "3 errors", saying the
Findings drawer holds the full text, for eight seconds. The drawer lists the window's
errors with the estate's findings, newest first and once — an error whose text a finding
already carries is not repeated — and "Clear the window's errors" empties the list. A
selected finding shows its whole text under its first line. Notices stay their own text
in a toast.

## Consequences

- No message is cut: every error is readable in full in the drawer, and selectable there.
- The toast no longer says what failed; the drawer's error badge and list do.
- The window's errors outlive the estate they were about until they are cleared or the
  window closes.
