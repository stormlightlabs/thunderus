# ChatGPT Codex Provider Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-03

## Objective

Add a `chatgpt-codex` provider that lets `thndrs` use Codex models through a
user's ChatGPT subscription credentials.

The provider must fit the existing provider architecture: model-prefix
selection, offline model-picker entries, the shared agent event loop,
provider-normalized streaming events, retry classification, and pure tests for
auth, request construction, stream parsing, and error mapping.

## Research Summary

OpenAI documents two Codex auth modes:

- ChatGPT sign-in for subscription access.
- API-key sign-in for usage-based access.

ChatGPT subscription access is not an OpenAI Platform API key. It is
OAuth/access-token auth attached to a ChatGPT account or workspace. OpenAI
documents Codex access tokens for trusted Business and Enterprise automation,
but still recommends API keys for ordinary programmatic CI/CD.

Reference implementations point to a consistent shape:

- Codex resolves credentials into bearer-header auth before model-provider
  requests.
- Codex stores local auth material in `auth.json` or an OS credential store and
  treats file storage as sensitive.
- Pi separates credential storage, OAuth refresh, request body construction,
  stream transport, and event normalization.
- Pi targets `https://chatgpt.com/backend-api/codex/responses` with
  Responses-like SSE or WebSocket streaming, bearer auth, account headers, and
  subscription-limit-aware error handling.
- Goose's OIDC proxy is useful as a security pattern for short-lived caller
  credentials, but it is not a ChatGPT subscription implementation.

## Decision Record

### Provider Identity

Use `chatgpt-codex/<model-id>` as the user-facing model prefix.

Initial known models:

- `chatgpt-codex/gpt-5.5`
- `chatgpt-codex/gpt-5.4`
- `chatgpt-codex/gpt-5.4-mini`
- `chatgpt-codex/gpt-5.3-codex-spark`

Keep `umans-coder` as the default model. ChatGPT Codex is selected only when
the user configures a `chatgpt-codex/` model id or chooses one in the model
picker.

### Endpoint

Use `https://chatgpt.com/backend-api/codex/responses` for the first
implementation.

This is the only endpoint found in the local references that maps ChatGPT
subscription credentials to a Responses-like Codex stream. It is not documented
as a stable Platform API endpoint, so the provider status and docs must label
this path as experimental and ChatGPT-backed.

### Authentication

Implement first-party ChatGPT login in `thndrs` rather than requiring users to
manually extract tokens.

Use device-code login as the primary interactive flow because it works in local
terminals, remote shells, and headless environments. Use the OpenAI Codex
device-code endpoints from the references:

- user code: `https://auth.openai.com/api/accounts/deviceauth/usercode`
- token polling: `https://auth.openai.com/api/accounts/deviceauth/token`
- verification URL: `https://auth.openai.com/codex/device`

Use browser PKCE login as the second supported login path when device-code login
is rejected by the server. The callback listener uses
`http://localhost:1455/auth/callback`, matching the Codex reference and Pi
reference.

Also support `CHATGPT_CODEX_ACCESS_TOKEN` for ephemeral automation and manual
debugging. When this env var is set, it overrides stored credentials for that
process and is never persisted.

### Credential Storage

Store ChatGPT Codex credentials in `~/.thndrs/auth.json` with file mode `0600`
on Unix.

The stored entry contains only the fields needed to refresh and derive request
auth:

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
provider request. Separate `thndrs` processes do not coordinate credential
refreshes with each other. Credential writes use an atomic temp-file replace,
and the last successful process write wins.

The implementation reads only `~/.thndrs/auth.json`. Codex's
`~/.codex/auth.json` belongs to Codex, contains sensitive credentials, and may
change independently of `thndrs`.

### Header Derivation

Keep token loading and refresh outside the provider request builder. The
provider consumes a pure auth result:

```rust
pub struct ChatGptCodexAuth {
    pub access_token: String,
    pub account_id: String,
}
```

Derive `account_id` from the JWT claim used by the Codex/Pi references:
`payload["https://api.openai.com/auth"].chatgpt_account_id`. If derivation
fails, the provider returns an auth error before any network request.

Attach these headers to SSE requests:

- `Authorization: Bearer <token>`
- `chatgpt-account-id: <account-id>`
- `originator: thndrs`
- `OpenAI-Beta: responses=experimental`
- `accept: text/event-stream`
- `content-type: application/json`

The header builder must be unit-tested without logging or snapshotting token
values.

### Transport

Implement SSE transport for the provider.

SSE is enough to validate auth, endpoint compatibility, Responses-like event
mapping, tool calls, usage, retry behavior, and transcript integration. The
acceptance criteria are defined against SSE.

### Request Body

Use a Responses-like body instead of chat completions:

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

Keep conversion local to `src/providers/chatgpt_codex.rs`. Do not introduce a
shared Responses helper in this feature.

### Stream Normalization

Normalize Responses-like SSE events into the existing `ProviderTurn` fields and
`AgentEvent` stream:

- assistant text deltas;
- visible reasoning deltas;
- function/tool call argument deltas;
- completed tool calls;
- usage;
- terminal status;
- provider errors.

Ignore encrypted reasoning payloads and any opaque backend-only fields. Session
records must continue to avoid full raw provider payloads.

### Error Handling

Use the existing `ProviderError` shape for missing credentials, invalid model
ids, HTTP transport failures, HTTP statuses, and JSON/protocol parse failures.

Retry:

- 429 throttling that is not a terminal quota/subscription failure;
- 500, 502, 503, and 504;
- network timeouts and connection resets.

Do not retry:

- missing credentials;
- invalid model ids;
- 401 or 403;
- terminal subscription, monthly usage, balance, or quota errors;
- malformed protocol events after a successful connection.

## Implementation Steps

1. Add `src/providers/chatgpt_codex.rs` with constants, model-prefix parsing,
   known models, auth structs, header building, request body building, SSE event
   parsing, retry classification, and unit tests.
2. Add a small auth-storage helper for `~/.thndrs/auth.json`, including Unix
   `0600` writes and redacted debug/status output.
3. Add device-code login and refresh-token exchange helpers.
4. Add browser PKCE login as fallback when device-code login is unavailable.
5. Register `ProviderKind::ChatGptCodex` in `src/agent.rs`.
6. Add `pub mod chatgpt_codex` in `src/providers/mod.rs`.
7. Add ChatGPT Codex models to the offline model picker in `src/app.rs`.
8. Add ignored live smoke tests that require a real ChatGPT subscription and
   network access.
9. Update provider, model, environment-variable, and configuration docs in the
   same implementation change.

## Security Requirements

- Secrets are accepted through environment variables or interactive login, not
  CLI flags.
- Stored credentials are written only under `~/.thndrs/auth.json`.
- File-backed credentials use `0600` on Unix.
- Token values are redacted from errors, debug output, tracing, snapshots, and
  session records.
- ChatGPT Codex credentials never silently fall back to `OPENAI_API_KEY`.
- `OPENAI_API_KEY` remains a separate usage-based provider credential.
- The provider status text identifies ChatGPT Codex as ChatGPT-backed and
  experimental.

## Files To Touch

- `src/providers/chatgpt_codex.rs`: new provider implementation.
- `src/providers/mod.rs`: export provider module.
- `src/agent.rs`: add `ProviderKind::ChatGptCodex` and route by model prefix.
- `src/app.rs`: include known ChatGPT Codex models in the offline picker.
- `src/cli.rs`: add `chatgpt-codex login` and `chatgpt-codex logout`
  subcommands.
- `docs/src/content/docs/guides/providers/chatgpt.md`: keep research and usage
  caveats current.
- `docs/src/content/docs/guides/usage/models.md`: document the new prefix.
- `docs/src/content/docs/guides/reference/environment-variables.md`: document
  `CHATGPT_CODEX_ACCESS_TOKEN`.
- `docs/src/content/docs/guides/reference/configuration.md`: document any new
  login command or config.

## Tests

Unit tests:

- model prefix parsing accepts `chatgpt-codex/gpt-5.5` and rejects raw model ids;
- request body strips the prefix and carries system instructions, user input,
  tool schemas, streaming flags, and Responses fields;
- auth loader reports missing credentials before network access;
- JWT account-id extraction succeeds for a fixture token and fails closed for a
  token without the expected claim;
- header builder attaches bearer and account headers without exposing secrets;
- credential storage writes redacted structures and uses Unix `0600` mode where
  supported;
- refresh logic refreshes expired credentials once per process under the mutex;
- SSE parser handles text deltas, tool-call deltas, usage, completion, backend
  errors, and malformed events;
- retry classifier separates terminal quota/subscription failures from
  transient throttling.

Ignored live tests:

- device-code login can obtain and persist credentials;
- text-only stream can answer a deterministic tiny prompt;
- one local tool-call turn can round-trip tool arguments and tool results;
- expired credentials refresh before a request.

Project verification after implementation:

```sh
cargo fmt
cargo clippy --fix --all-targets --allow-dirty
cargo clippy --all-targets
cargo test
```

## Acceptance Criteria

- `thndrs` can log in to ChatGPT Codex through device-code auth and persist a
  refreshable credential.
- Browser PKCE login works when device-code login is unavailable.
- `CHATGPT_CODEX_ACCESS_TOKEN` can run a one-off process without modifying
  credential storage.
- Selecting `chatgpt-codex/<model>` routes to the ChatGPT Codex provider.
- Missing or expired credentials produce clear provider errors and restore the
  prompt.
- Text streaming, visible reasoning, usage, and tool calls render through the
  existing transcript model.
- Pure tests pass without network.
- Live tests stay ignored by default and name their required env vars/account
  prerequisites.
- Session records and logs contain no bearer tokens, refresh tokens, or full raw
  backend payloads.

## Protocol Assertions

- The ChatGPT Codex backend endpoint is experimental because it is not
  documented as a stable Platform API endpoint.
- Personal ChatGPT Plus/Pro and Business/Enterprise accounts are both supported
  only through the same auth abstraction: bearer token plus account id.
- Requests use exactly the headers listed in this plan. Additional first-party
  headers are not added without a fixture and a plan update.
- Subscription-limit, balance, quota, and monthly usage errors are
  non-retryable and require fixture coverage for every observed backend string.

## Milestones

1. **Auth foundations:** credential file, redaction, JWT account-id extraction,
   device-code login, browser fallback, refresh, and pure auth tests.
2. **Provider foundations:** model prefix, known models, header builder, request
   body builder, SSE parser, retry classifier, and pure provider tests.
3. **Agent integration:** provider routing, model picker, transcript event
   mapping, and docs for model/env/config usage.
4. **Live validation:** ignored smoke tests for login, text streaming, tool
   calls, and refresh.
5. **Hardening:** fixture-driven error mapping for observed subscription limits,
   backend protocol drift, and redaction regressions.
