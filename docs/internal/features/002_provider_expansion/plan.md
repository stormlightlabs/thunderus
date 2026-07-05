# Provider Expansion Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-05

## Objective

Expand provider support around two practical user outcomes:

1. Support OpenCode Zen Big Pickle as the default free model option.
2. Support ChatGPT Codex through ChatGPT OAuth/subscription credentials.
3. Show client-observed time to first token (TTFT) in the TUI statusline.

The implementation must fit the existing provider architecture: model-prefix
selection, offline model-picker entries, first-run credential recovery, the
shared agent event loop, provider-normalized streaming events, retry
classification, and pure tests for request construction, stream parsing, error
mapping, and UI status rendering.

## Research Summary

OpenCode Zen is an optional OpenCode AI gateway with a curated model catalog.
Its docs currently list Big Pickle as raw model id `big-pickle`, using the
OpenAI-compatible endpoint:

```text
https://opencode.ai/zen/v1/chat/completions
```

OpenCode config-facing model ids use the `opencode/<model-id>` form, so the
`thndrs` model id should be `opencode/big-pickle`.

As of the 2026-07-05 capture:

- `GET https://opencode.ai/zen/v1/models` includes `big-pickle`.
- OpenCode's pricing table lists Big Pickle as free for input, output, and
  cached-read tokens.
- OpenCode says Big Pickle is free for a limited time.
- OpenCode says Big Pickle data may be used to improve the model during the
  free period.

TTFT is the elapsed time until the first generated output appears. In a local
TUI, `thndrs` can only measure client-observed TTFT: from local turn submission
to the first provider-normalized model-output event.

OpenAI documents Codex access through ChatGPT sign-in for subscription access
and API-key sign-in for usage-based access. ChatGPT subscription access is not
an OpenAI Platform API key; it is OAuth/access-token auth attached to a ChatGPT
account or workspace. Local reference implementations use bearer auth, account
headers, and a Responses-like stream against the ChatGPT backend Codex endpoint.

Research notes:

- `docs/src/content/docs/notebook/providers/opencode-zen.md`
- `docs/src/content/docs/notebook/ttft.md`
- `docs/src/content/docs/notebook/providers/protocols.md`
- `docs/src/content/docs/notebook/providers/opencode-go.md`

## Decision Record

### OpenCode Zen Provider Identity

Use `opencode/<model-id>` as the user-facing model prefix for general OpenCode
Zen models.

Initial supported model:

- `opencode/big-pickle`

Do not overload the existing `opencode-go/` prefix. OpenCode Go remains the
existing subscription provider backed by `https://opencode.ai/zen/go/v1`.
OpenCode Zen is a distinct provider backed by `https://opencode.ai/zen/v1`.

### ChatGPT Codex Provider Identity

Use `chatgpt-codex/<model-id>` as the user-facing model prefix for ChatGPT
subscription-backed Codex models.

Initial known models:

- `chatgpt-codex/gpt-5.5`
- `chatgpt-codex/gpt-5.4`
- `chatgpt-codex/gpt-5.4-mini`
- `chatgpt-codex/gpt-5.3-codex-spark`

ChatGPT Codex is not the default model. Users select it explicitly through
configuration or the model picker when they want ChatGPT subscription-backed
Codex access.

### Default Model Policy

Make `opencode/big-pickle` the default model.

The OpenCode docs describe Big Pickle as free for a limited time, but that
notice has reportedly been present for over a year. Treat the notice as an
important user-facing caveat, not as a reason to avoid making Big Pickle the
default.

Default behavior:

- compiled default model: `opencode/big-pickle`;
- first-run setup default provider: OpenCode Zen;
- missing Zen credential: recover through setup/login, then retry the same
  default model;
- model unavailable or removed from discovery: surface explicit provider
  recovery instead of silently changing the configured model;
- docs and setup copy identify both the free-period wording and the Big Pickle
  privacy caveat.

### OpenCode Zen Authentication

Add a distinct OpenCode Zen credential key.

Preferred environment variable:

```text
OPENCODE_ZEN_KEY
```

Do not silently reuse `OPENCODE_GO_KEY`. The Go provider and Zen provider may
share account infrastructure, but their product contracts and docs are
different enough that credential status should be explicit.

Credential input follows the existing API-key rules:

- accept through environment variables or interactive setup/login;
- do not accept through CLI flags;
- store only in the existing managed credential file path used by setup/login;
- redact key values from errors, debug output, snapshots, and session records.

### ChatGPT Codex Authentication

Implement first-party ChatGPT login rather than requiring users to manually
extract tokens.

Use device-code login as the primary interactive flow because it works in local
terminals, remote shells, and headless environments. Use these Codex auth
endpoints from the references:

- user code: `https://auth.openai.com/api/accounts/deviceauth/usercode`
- token polling: `https://auth.openai.com/api/accounts/deviceauth/token`
- verification URL: `https://auth.openai.com/codex/device`

Use browser PKCE login as the fallback when device-code login is unavailable.
The callback listener uses `http://localhost:1455/auth/callback`.

Also support `CHATGPT_CODEX_ACCESS_TOKEN` for ephemeral automation and manual
debugging. When this env var is set, it overrides stored credentials for that
process and is never persisted.

Store ChatGPT Codex credentials in `~/.thndrs/auth.json` with file mode `0600`
on Unix. The stored entry contains only fields needed to refresh and derive
request auth:

```json
{
  "chatgpt_codex": {
    "access_token": "...",
    "refresh_token": "...",
    "expires_at_ms": 1780000000000,
    "account_id": "..."
  }
}
```

Refresh expired credentials under a process-local mutex before sending the
provider request. Credential writes use an atomic temp-file replace, and the
last successful process write wins.

Derive `account_id` from the JWT claim used by the Codex/Pi references:
`payload["https://api.openai.com/auth"].chatgpt_account_id`. If derivation
fails, the provider returns an auth error before any network request.

### OpenCode Zen Endpoint And Request Body

For `opencode/big-pickle`, strip the `opencode/` prefix and send raw model id
`big-pickle` to:

```text
POST https://opencode.ai/zen/v1/chat/completions
```

Use the existing OpenAI-compatible chat-completions request shape already used
by the OpenCode Go OpenAI-compatible route where possible:

- `model`;
- `messages`;
- `tools` when local tools are available;
- `stream: true`;
- provider-appropriate max-token field;
- no provider-native web search unless explicitly added later.

Do not implement all Zen endpoint families in the first pass. The first pass
supports Big Pickle and the OpenAI-compatible chat-completions route. Add
Anthropic Messages, Responses, and Google model routes only after fixture
captures prove their streaming and tool-call shapes.

### ChatGPT Codex Endpoint And Request Body

Use the ChatGPT backend Codex Responses endpoint:

```text
POST https://chatgpt.com/backend-api/codex/responses
```

This endpoint is not documented as a stable OpenAI Platform API endpoint, so
the provider status and docs must label ChatGPT Codex as ChatGPT-backed and
experimental.

Attach these headers to streaming requests:

- `Authorization: Bearer <token>`
- `chatgpt-account-id: <account-id>`
- `originator: thndrs`
- `OpenAI-Beta: responses=experimental`
- `accept: text/event-stream`
- `content-type: application/json`

Use a Responses-like streaming body:

```json
{
  "model": "gpt-5.5",
  "store": false,
  "stream": true,
  "instructions": "<system prompt>",
  "input": [],
  "tools": [],
  "tool_choice": "auto",
  "parallel_tool_calls": true,
  "text": { "verbosity": "low" },
  "include": ["reasoning.encrypted_content"]
}
```

Keep conversion local to the ChatGPT Codex provider module. Do not introduce a
shared Responses helper until another stable provider needs it.

### Model Discovery

Use `GET https://opencode.ai/zen/v1/models` as a lightweight model-list
validation and picker-refresh hook.

The endpoint currently returns basic model ids and ownership only. Because it
does not expose free/paid status or privacy caveats, the implementation must
not infer pricing from model discovery alone. Pricing and privacy copy remains
documentation-backed until OpenCode exposes machine-readable metadata.

### Stream Normalization

Normalize OpenAI-compatible chat-completions streaming into the existing
`ProviderTurn` fields and `AgentEvent` stream:

- assistant text deltas;
- reasoning deltas if present in a compatible field;
- function/tool call argument deltas;
- completed tool calls;
- usage;
- terminal status;
- provider errors.

Reuse existing OpenCode Go parsing behavior when the event shape matches. Add
fixtures for Big Pickle-specific deltas before special-casing any behavior.

### Error Handling

Use the existing `ProviderError` shape for missing credentials, invalid model
ids, HTTP transport failures, HTTP statuses, and JSON/protocol parse failures.

Retry:

- 429 throttling that is not a terminal balance/usage failure;
- 500, 502, 503, and 504;
- network timeouts and connection resets.

Do not retry:

- missing credentials;
- invalid model ids;
- 401 or 403;
- terminal balance, monthly usage, unavailable model, subscription, quota, or
  ended-free-period errors;
- malformed protocol events after a successful connection.

### TTFT Measurement

Measure TTFT as client-observed latency:

```text
turn submitted locally -> first semantic model-output event
```

The first semantic model-output event is the first of:

- assistant text delta;
- visible reasoning delta;
- tool-call delta or completed tool call.

Provider status messages, retry notices, setup prompts, and usage-only events
do not stop the TTFT timer.

When a retry happens, report user-observed TTFT across the whole turn rather
than per-attempt TTFT. Per-attempt timing can be added later to verbose logs if
needed.

### Statusline Rendering

Show TTFT in the TUI statusline without transcript noise.

Target states:

- before first output during an active run: `ttft: pending`;
- after first output during an active run: `ttft: 842ms` or `ttft: 1.4s`;
- after the run finishes: retain the last turn's TTFT until the next turn
  starts;
- if the turn ends without semantic model output: show no TTFT value or a
  clearly non-misleading empty state.

Width handling must preserve the existing statusline hierarchy. On narrow
screens, TTFT can be hidden before model, run status, and prompt affordances.

### ChatGPT Codex Scope

ChatGPT Codex OAuth/subscription integration is in scope for this provider
expansion. It is separate from the OpenCode Zen default path, but it should be
implemented under the same expansion feature because the work shares provider
routing, model picker, stream normalization, retry classification, docs, and
statusline observability.

## Implementation Steps

1. Add OpenCode Zen credential plumbing with `OPENCODE_ZEN_KEY`, setup/login
   support, doctor status, and redacted output.
2. Add `src/core/providers/opencode_zen.rs` with constants, prefix parsing,
   Big Pickle known model metadata, request body construction, stream parsing,
   model discovery, validation, retry classification, and tests.
3. Register `ProviderKind::OpenCodeZen` and route `opencode/` model ids to the
   Zen provider.
4. Add `opencode/big-pickle` to the model picker and first-run provider choices.
5. Update setup defaults so Big Pickle is the default free setup option.
6. Add ChatGPT Codex credential storage, device-code login, browser PKCE
   fallback, refresh, logout, and redaction.
7. Add `src/core/providers/chatgpt_codex.rs` with prefix parsing, known models,
   auth header construction, Responses request construction, stream parsing,
   retry classification, and tests.
8. Register `ProviderKind::ChatGptCodex` and route `chatgpt-codex/` model ids
   to the provider.
9. Add ChatGPT Codex models to the offline model picker and docs.
10. Add TTFT state to the app model and set it from agent-event handling.
11. Render TTFT in the statusline with width-aware hiding and snapshot tests.
12. Add ignored live smoke tests for Zen model listing, Big Pickle streaming,
    ChatGPT Codex login, ChatGPT Codex streaming, and refresh.
13. Update provider, model, environment-variable, and setup docs.

## Security Requirements

- Zen API keys are accepted through environment variables or interactive
  setup/login, not CLI flags.
- Zen API keys are redacted from errors, debug output, snapshots, session
  records, and verbose provider status rows.
- `OPENCODE_ZEN_KEY` is separate from `OPENCODE_GO_KEY`.
- OpenCode Go keeps using `OPENCODE_GO_KEY` and `opencode-go/` model ids.
- ChatGPT Codex credentials never silently fall back to `OPENAI_API_KEY`.
- `OPENAI_API_KEY` remains separate from ChatGPT subscription credentials.
- ChatGPT Codex access tokens and refresh tokens are redacted from errors,
  debug output, tracing, snapshots, and session records.
- Big Pickle docs and setup copy identify the free-period privacy caveat.
- Session records do not store raw provider payloads.
- TTFT records do not include prompt or model-output content.

## Files To Touch

- `src/core/providers/opencode_zen.rs`: new provider implementation.
- `src/core/providers/chatgpt_codex.rs`: new ChatGPT Codex provider
  implementation.
- `src/core/providers/mod.rs`: export provider module.
- `src/core/agent.rs`: add `ProviderKind::OpenCodeZen`,
  `ProviderKind::ChatGptCodex`, and route by prefix.
- `src/core/auth.rs`: add `OPENCODE_ZEN_KEY` and ChatGPT Codex credential
  handling.
- `src/cli/app.rs`: add model picker item, login/logout routing, TTFT state,
  and agent-event timer updates.
- `src/cli/commands/setup.rs`: add OpenCode Zen as the default setup provider.
- `src/cli/commands/doctor.rs`: report OpenCode Zen credential status.
- `src/cli/commands/auth.rs`: validate/store OpenCode Zen API keys and manage
  ChatGPT Codex login/logout.
- `src/cli/renderer/live.rs`: render TTFT in the statusline.
- `docs/src/content/docs/providers/opencode-zen.md`: document usage.
- `docs/src/content/docs/providers/chatgpt.md`: document ChatGPT Codex usage
  and caveats.
- `docs/src/content/docs/usage/models.md`: document the `opencode/` prefix.
- `docs/src/content/docs/reference/environment-variables.md`: document
  `OPENCODE_ZEN_KEY` and `CHATGPT_CODEX_ACCESS_TOKEN`.
- `docs/src/content/docs/reference/configuration.md`: document default model
  behavior.
- `docs/src/content/docs/notebook/providers/opencode-zen.md`: keep research
  current.
- `docs/src/content/docs/notebook/ttft.md`: keep TTFT research current.

## Tests

Unit tests:

- model prefix parsing accepts `opencode/big-pickle` and rejects raw
  `big-pickle`;
- provider routing selects `ProviderKind::OpenCodeZen` for `opencode/`;
- missing credentials fail before network access;
- credential redaction covers environment, setup, doctor, and provider errors;
- request body strips the prefix and carries messages, tools, streaming flags,
  and token limits;
- model discovery maps `big-pickle` to `opencode/big-pickle`;
- stream parser handles text deltas, tool-call deltas, usage, completion,
  backend errors, and malformed events;
- retry classifier separates transient throttling from terminal balance,
  unavailable-model, and free-period-ended errors;
- model picker includes Big Pickle with concise free/caveat text;
- setup default/recovery behavior is deterministic;
- TTFT starts on local submit and stops on first semantic model-output event;
- status-only and usage-only events do not stop TTFT;
- retry preserves one user-observed TTFT timer across attempts;
- statusline snapshots cover pending, measured, retained, and narrow widths.
- ChatGPT Codex prefix parsing accepts `chatgpt-codex/gpt-5.5` and rejects raw
  model ids;
- ChatGPT Codex missing credentials fail before network access;
- JWT account-id extraction succeeds for a fixture token and fails closed for a
  token without the expected claim;
- ChatGPT Codex credential storage writes redacted structures and uses Unix
  `0600` mode where supported;
- ChatGPT Codex env token override does not write credential storage;
- ChatGPT Codex header construction attaches bearer and account headers without
  exposing secrets;
- ChatGPT Codex Responses request construction covers text-only and tool turns;
- ChatGPT Codex stream parser handles text deltas, visible reasoning deltas,
  tool-call deltas, usage, completion, backend errors, and malformed events.

Ignored live tests:

- Zen model discovery includes or gracefully lacks `big-pickle`;
- Big Pickle can answer a deterministic tiny prompt;
- Big Pickle can stream a local tool-call round trip if tool calls are
  supported;
- ChatGPT Codex device-code login can obtain and persist credentials;
- ChatGPT Codex browser fallback login works when device-code login is
  unavailable;
- ChatGPT Codex text-only streaming can answer a deterministic tiny prompt;
- ChatGPT Codex local tool-call round trip works;
- ChatGPT Codex expired credentials refresh before a request;
- live tests name `OPENCODE_ZEN_KEY`, network access, limited-free pricing, and
  privacy prerequisites where relevant;
- ChatGPT Codex live tests name their ChatGPT subscription/account
  prerequisites.

Project verification after implementation:

```sh
cargo fmt
cargo clippy --fix --all-targets --allow-dirty
cargo clippy --all-targets
cargo test opencode_zen
cargo test chatgpt_codex
cargo test provider
cargo test cli
cargo test
```

## Acceptance Criteria

- `opencode/big-pickle` routes to OpenCode Zen and sends raw model
  `big-pickle` to `/zen/v1/chat/completions`.
- Missing `OPENCODE_ZEN_KEY` produces a clear setup/recovery path.
- First-run setup presents Big Pickle as the default free option with
  limited-free and privacy caveats.
- Existing OpenCode Go behavior remains unchanged.
- `chatgpt-codex/<model>` routes to the ChatGPT Codex provider.
- ChatGPT Codex can log in through device-code auth and persist a refreshable
  credential.
- Browser PKCE login works when ChatGPT Codex device-code login is unavailable.
- `CHATGPT_CODEX_ACCESS_TOKEN` can run a one-off process without modifying
  credential storage.
- The statusline shows pending, measured, and retained TTFT without wrapping or
  overlapping at supported widths.
- TTFT is measured from local submit to first semantic model-output event.
- Pure tests pass without network.
- Live tests stay ignored by default and identify their external
  prerequisites.
- Session records and logs contain no API keys or raw provider payloads.

## Protocol Assertions

- `opencode/` means OpenCode Zen, not OpenCode Go.
- `opencode-go/` keeps its existing Go semantics.
- `chatgpt-codex/` means ChatGPT subscription-backed Codex, not OpenAI
  Platform API-key access.
- ChatGPT Codex uses bearer token plus ChatGPT account id auth.
- The ChatGPT backend Codex endpoint is experimental because it is not
  documented as a stable Platform API endpoint.
- `big-pickle` is the default despite the docs' limited-free wording.
- Big Pickle privacy behavior during the free period must be visible in setup
  and docs.
- TTFT is a client-observed UX metric unless a provider exposes server-side
  timing metadata.

## Milestones

1. **Zen foundations:** credential key, provider module, prefix parsing, Big
   Pickle request body, model discovery, and unit tests.
2. **Zen integration:** provider routing, setup/login/doctor, model picker,
   docs, ignored live tests, and default recovery behavior.
3. **ChatGPT Codex auth:** credential file, redaction, JWT account-id
   extraction, device-code login, browser fallback, refresh, logout, and pure
   auth tests.
4. **ChatGPT Codex provider:** prefix parsing, known models, header builder,
   Responses request body, SSE parser, retry classifier, provider routing, and
   docs.
5. **TTFT state:** app timing fields, event handling, retry behavior, and unit
   tests.
6. **TTFT rendering:** statusline layout, width hiding, and renderer snapshots.
7. **Hardening:** observed error fixtures, setup copy refinements, docs updates,
   privacy/default review, and ChatGPT backend protocol drift checks.
