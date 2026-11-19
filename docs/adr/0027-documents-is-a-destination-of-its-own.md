# 0027 — Documents is a destination of its own, and the rail is full

- **Status:** accepted; the rail is no longer full — Checks and Deploy became one destination ([ADR 0028](0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md))
- **Date:** 2026-11-18
- **Deciders:** the maintainer

## Context

The decisions sheet — every decision an estate rests on, as text, markdown, PDF, JSON or
a workbook — had a card in two views: at the foot of Decisions as the "Sign-off sheet"
and at the foot of the Overview as the "Handover record". It is one document, written by
one command, under two titles. In both views the card was a wide area that belonged to
neither: Decisions is the walk through the questions, the Overview is what the estate is
and what it owes. More documents an estate hands to people are expected.

The rail held six primary destinations, which was this app's own limit
([`docs/ui.md`](../ui.md), the navigation rail's anatomy); Material 3 puts three to
seven in a navigation rail.

## Options

1. **A seventh primary destination, Documents.** Cost: the rail is full for an
   organisation's estate; the next view is a tab or replaces one.
2. **Documents in the rail's footer**, beside Agent and Settings. Cost: the footer holds
   what is not about the estate's content, and a document is.
3. **A tab of Deploy.** Cost: not a place of its own, and Deploy becomes a mixed view.

For the card: one card, or the two titles side by side.

## Decision

- **Documents is the seventh primary destination**, last in the rail: what is handed to
  people once the rest is settled. The app's limit is Material 3's, seven.
- **One card, "Decisions sheet".** Its text says both uses — signed before an
  organisation is touched, kept as the record after the rollout. The card is gone from
  Decisions and from the Overview.
- The compliance evidence report stays in Checks, where its checks are.

## Consequences

- An organisation's estate shows seven primary destinations, an estate in a delegated
  place without a preset library six. At a low window the destinations scroll; the
  footer stays.
- An eighth primary destination is not available: it is a navigation drawer's. A new
  document is a card in Documents.
- Decisions ends with its question list, and the Overview with its command log.
