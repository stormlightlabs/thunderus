---
title: "Providers"
---

This page explains the provider-neutral runtime boundary and the work owned by
each provider adapter.

## Mental Model

A provider adapter is the narrow edge between the agent loop and a model
service. The agent loop supplies normalized messages, the shared tool catalog,
model-specific request settings, and per-run Responses continuation state. The
adapter authenticates, converts that input into one wire request, sends an SSE
request, and identifies which parser the runtime should use. Provider-specific
JSON does not cross this boundary: the rest of the run consumes normalized
`AgentEvent` values.

```text
agent messages + tools + model settings
                  │
                  ▼
       StreamingProvider adapter
       auth → request conversion → HTTP/SSE
                  │
                  ▼
   StreamFormat → normalized agent events
```

The current built-in providers are ChatGPT Codex, OpenCode Zen, and OpenCode
Go. OpenCode Go and Zen share the OpenAI, Anthropic, and Responses conversion
helpers where their wire contracts match, but retain separate endpoint and
model policies.

## Responsibilities

- The provider trait defines metadata loading, request serialization and
  dispatch, stream-format selection, token budgeting, and retry/error policy.
- Each adapter owns its base URL, credential name, model-id prefix, endpoint
  selection, headers, request body, and model metadata projection.
- The agent loop owns tool execution, cancellation signaling, transcript state,
  and conversion of parsed provider chunks into application events.
- The shared transport bounds DNS/connect, request send, response start, and
  idle stream reads without imposing a total lifetime on a healthy stream.

## Request Conversion

The agent loop builds provider-neutral `ProviderMessage` values and one tool
catalog. `StreamingRequest` also carries the token budget, reasoning controls,
reasoning-summary preference, and provider-private continuation state.

| Provider      | Model prefix     | Request routes                                                                                   | Conversion                                                 |
| ------------- | ---------------- | ------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| ChatGPT Codex | `chatgpt-codex/` | Responses                                                                                        | Codex Responses body and SSE contract                      |
| OpenCode Zen  | `opencode/`      | Responses for `gpt-*`; Messages for `claude-*`/`qwen*`; otherwise chat completions               | Codex, Anthropic, or OpenAI helper at the adapter boundary |
| OpenCode Go   | `opencode-go/`   | Responses for `grok-*` and `gpt-*`; Messages for `minimax-*`/`qwen*`; otherwise chat completions | Same shared helpers, with Go's `/zen/go/v1` base URL       |

OpenCode Go's documented model set includes Grok 4.5, GPT 5.6 Luna, GLM-5.3,
GLM-5.2, GLM-5.1, Kimi K3, Kimi K2.7 Code, Kimi K2.6, MiMo-V2.5,
MiMo-V2.5-Pro, MiniMax M3/M2.7/M2.5, Qwen3.8 Max, Qwen3.7 Max,
Qwen3.7 Plus, Qwen3.6 Plus, DeepSeek V4 Pro, DeepSeek V4 Flash, and Hy3.
The app-facing ids use `opencode-go/<model-id>`; the API receives the raw id.
The live `/models` response is still loaded for picker metadata and validation.

## Reasoning controls

`providers::reasoning_options` owns the OpenCode Go capability profile because
Go's `/models` endpoint returns ids but no reasoning metadata. `Auto` is always
available. The adjustable profiles are:

- Grok 4.5: `low`, `medium`, `high`.
- GPT 5.6 Luna: `none`, `low`, `medium`, `high`, `xhigh`, `max`.
- GLM-5.3: `low`, `high`, `max`; GLM-5.2 and DeepSeek V4 Pro: `high`, `max`.
- DeepSeek V4 Flash: `low`, `high`, `max`; Kimi K3: `max`.
- MiniMax M3: `none`, `on`; Hy3: `none`, `low`, `high`.
- Qwen3.8 Max, Qwen3.7 Max, Qwen3.7 Plus, and Qwen3.6 Plus: `high`, `max`.

GLM-5.1, Kimi K2.7 Code, Kimi K2.6, MiMo-V2.5, MiMo-V2.5-Pro, MiniMax
M2.7, and MiniMax M2.5 currently expose only `Auto`. Unknown Go ids also fall
back to `Auto` until a capability profile is added.

The adapter rejects a setting outside the selected model's profile before it
serializes a request, then lowers the accepted setting by endpoint family:

- Responses routes use the shared Codex builder and send `reasoning.effort`,
  with `reasoning.summary = "auto"` when summaries are enabled.
- OpenAI-compatible chat routes send top-level `reasoning_effort`.
- Anthropic-compatible Messages routes send `thinking`. MiniMax `none` and
  `on` become `disabled` and `adaptive`; Qwen `high` and `max` become enabled
  thinking with request-sized `budget_tokens` values. `high` uses half of the
  request output budget, and `max` uses that budget minus one token so the
  thinking budget stays below `max_tokens`.

The application carries only `ReasoningEffort` and `ReasoningSummary`. These
provider fields stay inside the adapter.

## Streaming Event Normalization

`stream_format` selects one of three parsers: Anthropic Messages, OpenAI chat
completions, or ChatGPT-compatible Responses. Those parsers normalize text and
reasoning deltas, usage, tool-call lifecycle, provider errors, and completion
markers into the agent event vocabulary. Responses continuation items remain
in `ProviderContinuation` for the active run only; they are not session or
public-library data.

## Authentication

`from_env_or_dotenv` resolves the provider credential through the shared auth
resolver. Environment variables take precedence over the workspace `.env` and
managed credential stores. The provider setup and login commands use the same
provider metadata and credential names. `OPENCODE_GO_KEY` authenticates Go and
`OPENCODE_ZEN_KEY` authenticates Zen; missing credentials produce an actionable
setup/login error. Credential validation performs a lightweight authenticated
`GET /models` request.

## Errors, Retries, and Cancellation

HTTP status, transport, authentication, model-id, and JSON failures are
represented by `ProviderError`. A 401/403 is a credential rejection, 429 is a
rate-limit failure, and 5xx responses are server failures. Status 429 and 5xx,
plus non-aborted transport failures, are retryable; missing credentials,
invalid model ids, authentication rejection, and JSON failures are not.
Adapters provide user-facing failure text without exposing provider payloads
unnecessarily.

Cancellation is owned by the agent run. It stops the provider read/request
cooperatively; the shared transport also prevents an indefinitely stalled
connection through its response and idle-read timeouts. A cancelled or aborted
transport error is not retried.

## Boundaries

- `core/providers` owns provider wire protocols, HTTP, auth lookup, metadata,
  endpoint policy, and provider error classification.
- `core/agent` owns the provider-neutral turn, stream parsing dispatch, tools,
  retries, cancellation token, and normalized events.
- `cli/app` and renderers consume events and status; they do not construct wire
  payloads or parse provider JSON.
- `core/session` persists normalized run records and accounting, not API keys,
  raw provider continuation state, or provider-specific wire contracts.

## Key Types

- `StreamingProvider` — adapter contract used by the agent loop.
- `StreamingRequest` — normalized per-request settings and tool catalog.
- `ProviderMessage` and `ProviderContentBlock` — provider-neutral messages.
- `ProviderContinuation` — in-memory Responses history for one active run.
- `StreamFormat` — parser selection for Anthropic, OpenAI chat, or Responses.
- `ProviderError` — shared transport, status, auth, model, and JSON failures.
- `OpenCodeGoClient`, `OpenCodeZenClient`, and `ChatGptCodexClient` — concrete adapters.

## Invariants

- Every configured model uses the prefix and provider route that owns it.
- The serialized request used for accounting is the same body sent by the
  adapter.
- Tool schemas are converted only at the provider boundary.
- An SSE stream is parsed according to the selected endpoint family.
- API credentials and Responses continuation payloads do not enter public
  library contracts or persisted session records.
- Only retryable failures are retried, and cancellation/abort failures are not.
- Model discovery may add live entries, while curated known models keep setup
  and picker behavior useful when discovery is unavailable.

## Source Map

| Responsibility                                       | Primary source                                               |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| Provider trait, errors, timeouts, and stream formats | `crates/thndrs/src/core/providers/mod.rs`                    |
| OpenAI and Anthropic request conversion/parsing      | `crates/thndrs/src/core/providers/openai.rs`, `anthropic.rs` |
| Responses conversion/parsing                         | `crates/thndrs/src/core/providers/codex.rs`                  |
| OpenCode provider registry                           | `crates/thndrs/src/core/providers/opencode.rs`               |
| OpenCode Go adapter and model list                   | `crates/thndrs/src/core/providers/opencode/go.rs`            |
| OpenCode Zen adapter and model list                  | `crates/thndrs/src/core/providers/opencode/zen.rs`           |
| Provider selection and run retries                   | `crates/thndrs/src/core/agent/`                              |
| Credential resolution and storage                    | `crates/thndrs/src/core/auth/`                               |
| Provider setup/login commands                        | `crates/thndrs/src/cli/commands/setup.rs`, `auth.rs`         |

## Related

- [Runtime and state](/docs/internals/runtime/)
- [Request lifecycle](/docs/internals/lifecycle/)
- [Context assembly](/docs/internals/context/)
- [Tools](/docs/internals/tools/)
- [Sessions](/docs/internals/sessions/)
- [Adding a provider](/docs/development/adding-a-provider/)
