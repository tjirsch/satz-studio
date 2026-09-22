# 0004 — Claude natively, other providers adapt into its message model

- **Status:** superseded by [ADR 0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md) — the app runs no model: the message model, the client and the provider adapters are gone with the chat
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

satz-studio drives satz through a model: the satz MCP tools are the model's tools, a
turn streams into the Chat view, and every tool call passes the app's approval gate
(ADR 0005). The app has to support Claude fully — streaming, adaptive thinking, the
effort setting, prompt caching with breakpoints, tool use, server-side refusal
fallbacks (ADR 0009) — and other providers configurably, so an estate can be worked on
with a local model where no API key may be used.

There is no official Rust SDK for the Claude API. The official SDKs are Python,
TypeScript, Java, Go, Ruby, C# and PHP, plus the `ant` CLI; a Rust program speaks the
Messages API itself or through a third party. The question is which types the app's
chat is written against, and where the other providers plug in.

## Decision

The Messages API over HTTPS, spoken directly. `ClaudeClient`
(`crates/satz-studio-core/src/llm/claude/client.rs`) posts to `{base_url}/v1/messages`
with `reqwest`, `anthropic-version: 2023-06-01`, the credential as `x-api-key` or a
bearer token, and the betas a request needs joined into one `anthropic-beta` header.
`SseDecoder` and `Assembler` (`claude/sse.rs`) turn the event stream into
`StreamEvent`s and a `Response`.

The wire types are the app's only message model (`claude/types.rs`): a transcript is a
`Vec<Message>` of `ContentBlock`s, a request is `Request`, and `body()` renders it —
tools sorted by name, then system, then messages — placing the cache breakpoints
itself (the last tool, `system[0]`, the last block of the last user message), sending
adaptive thinking with a summarised display, `output_config.effort`, and
`fallbacks: "default"` when asked. `ContentBlock` types the five block kinds the code
reads; every other block is `ContentBlock::Other(Value)`, carried and serialised
verbatim.

Other providers implement `ChatProvider` (`llm/mod.rs`): `stream(&Request, tx,
cancel)` maps the Claude-shaped request into the provider's wire format and its stream
back into `StreamEvent`s. `OpenAiCompat` (`{base_url}/chat/completions`) and `Ollama`
(`{base_url}/api/chat`, NDJSON) drop thinking, effort and cache breakpoints and say so
through `Capabilities { tools, thinking, effort, cache_control }`, which the Chat view
shows. Claude never passes through a mapping.

## Consequences

- The SSE decoder, the assembler and the mapping from an HTTP status to `ClaudeError`
  are the app's to maintain.
- A stream that started is never restarted. A request is retried at most twice — on a
  rate limit, an overload, a server error or a connection failure — and only before
  the first byte of its stream; after that byte, a failure ends the turn.
- `ContentBlock::Other` replays a block this code does not read — a `fallback` block,
  a kind added after this code was written — exactly as the API returned it, so a
  saved transcript survives an API addition and the next request echoes it.
- The other providers are lossy by construction: an Ollama tool call has no id, so the
  adapter numbers them; a tool result's error flag becomes a prefix on its text; a
  `content_filter` finish is a refusal with no fallback behind it.
- A new Claude feature is one change in `types.rs` and `body()`; a new provider is one
  `ChatProvider` implementation and one `ProviderChoice` variant in Settings.

## Pros and cons of the options

### A — the Messages API directly, the wire types as the message model *(chosen)*

- **Good:** every feature of the API is available the day it exists — thinking blocks
  with their signatures, breakpoints, `stop_details`, the fallback beta — with no layer
  between the app and the wire; the transcript is what the API saw.
- **Bad:** an HTTP client, an SSE decoder and an error table to maintain; an API change
  reaches the app as a test that fails.

### B — a community Rust SDK crate

- **Good:** the request and response types, the streaming and the retries come from a
  crate; less code in the app.
- **Bad:** unofficial and behind the API — adaptive thinking, effort, the fallback beta
  and `stop_details` each arrive when a maintainer gets to them; its types become the
  app's model, and a field it lacks is a feature the app cannot use.

### C — a generic LLM abstraction crate, Claude as one backend

- **Good:** other providers come free, behind one trait.
- **Bad:** the lowest common denominator: thinking, effort, prompt caching and refusal
  handling have no place in a model shaped for every provider, and the transcript
  loses the blocks the next request must echo (a thinking block's signature).

### D — the Claude Code CLI as the backend

`claude -p --output-format stream-json`, authenticated by the subscription.

- **Good:** no API key, no HTTP client; the CLI's own agent loop and its permissions.
- **Bad:** the app no longer owns tool execution: the CLI would call satz's MCP server
  itself, outside the app's approval gate, the estate's write lock and its identity per
  estate. Kept as a second backend behind `ChatProvider` for a later version.
