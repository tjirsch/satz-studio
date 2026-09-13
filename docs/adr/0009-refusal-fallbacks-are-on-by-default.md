# 0009 — refusal fallbacks are on by default

- **Status:** accepted
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

Claude's safety classifiers can decline a request. The response then ends with
`stop_reason: "refusal"` and a `stop_details` object carrying a category, an
explanation and, when there is one, a recommended model; the content before it is
partial. The estates this app edits are about organisation policies, CIS controls,
IAM grants, firewall rules and service-account keys — material a classifier can read
as an attack on an organisation rather than the administration of one.

The API offers server-side fallbacks: with `fallbacks: "default"` and the beta header
`server-side-fallback-2026-07-01`, a request the classifier declines is re-run on a
fallback model inside the same call, and the response says so with a `fallback` block
(`from`, `to`) and its `model` field. The question is the default, and what the app
does with a refusal that survives it.

## Decision

Fallbacks are on by default. `Settings.fallbacks` (`settings.rs`) and
`Agent.fallbacks` (`llm/agent/mod.rs`) start `true`; `body()` in `claude/types.rs`
sends `fallbacks: "default"` and `betas()` adds `BETA_FALLBACK` to the
`anthropic-beta` header while the flag is set. A switch in Settings turns it off.

A refusal that survives the fallback ends the turn: `Agent::turn` maps
`StopReason::Refusal` to `ClaudeError::Refused { category, explanation,
recommended_model }` from `stop_details`; `run_turn` truncates the transcript to where
the turn began, so the next request is valid, and raises `AgentEvent::Refused` for the
Chat view to show with the category and the explanation. The app retries nothing on
its own; when `stop_details` names a recommended model, the view offers a retry on it
as an action the operator takes.

## Consequences

- A turn may be served by a different model than the one selected. `Response.model`
  says which, and the `fallback` block travels in the transcript as
  `ContentBlock::Other`, verbatim, so the next request echoes it.
- The beta header is sent on every request while the setting is on, whether or not a
  fallback happens.
- A fallback re-runs the generation inside the call, so a declined request costs the
  time and tokens of two.
- The setting means nothing to the other providers (ADR 0004): a `content_filter`
  finish maps to `StopReason::Refusal` with no fallback behind it, and the same
  discard-and-show path handles it.
- A refusal never leaves a partial assistant message in the transcript, so a
  `tool_use` the model started before the refusal is not executed.

## Pros and cons of the options

### A — on by default, a Settings switch off *(chosen)*

- **Good:** a refusal in a chat about the material the app exists for is handled where
  it arises, in one call, and the operator sees which model answered.
- **Bad:** the selected model is a preference, not a guarantee; a beta header on every
  request; the cost of a second generation when the classifier fires.

### B — off by default

- **Good:** the selected model is the model, always; no beta in the request.
- **Bad:** every refusal is a dead end. The operator reads a category and an
  explanation, turns a setting on and asks again — for a request that was about their
  own organisation's policies.

### C — the app's own retry on another model

- **Good:** no beta; the app chooses the fallback model and can show the switch.
- **Bad:** a second request the operator did not make, and the transcript would carry
  two attempts — or the app would hide one, and the transcript would no longer be what
  the API saw. The server-side path is one call and one answer.
