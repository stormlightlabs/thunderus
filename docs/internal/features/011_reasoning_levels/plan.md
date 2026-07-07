# Reasoning Levels

Status: Draft
Captured: 2026-07-07

## Source Notes

This plan is based on the current provider and configuration shape in:

- `src/core/tools.rs`
- `src/core/agent.rs`
- `src/core/providers/mod.rs`
- `src/core/providers/umans.rs`
- `src/core/providers/openai.rs`
- `src/core/providers/anthropic.rs`
- `src/core/providers/opencode/go.rs`
- `src/core/providers/opencode/zen.rs`
- `src/core/providers/codex.rs`
- `src/server/config_options.rs`
- `src/core/config/mod.rs`
- `src/cli/mod.rs`

API context:

- OpenAI Responses API exposes `reasoning.effort` with model-dependent values
  such as `none`, `minimal`, `low`, `medium`, `high`, and `xhigh`.
- Anthropic-compatible models expose reasoning through provider-specific
  `thinking` or adaptive-effort controls, depending on model family.
- Umans live model metadata already includes
  `capabilities.reasoning { supported, can_disable, levels, default_level }`.
- ACP session config options are the right server-side surface for model config
  such as model, web search, and reasoning level.

## Problem

`thndrs` can display streamed reasoning output from supported providers, but
users cannot control the reasoning budget or effort level for a run.

Today reasoning behavior is implicit:

- `model` is a single string, so users cannot choose reasoning effort without
  changing model ids or relying on provider defaults;
- `AgentRunConfig` carries `model` and `search_mode`, but no reasoning setting;
- providers build request bodies independently, so adding ad hoc reasoning
  fields risks inconsistent behavior;
- the ACP server exposes `model` and `websearch` config options, but not
  reasoning;
- Umans metadata already describes reasoning capability, but the client does
  not consume it for request validation or UI.

The feature needs a small, provider-neutral user contract with provider-specific
serialization at the boundary.

## Milestone Outcome

A user can choose a reasoning level with config, CLI, and ACP session config.
The default `auto` preserves current behavior by sending no explicit reasoning
override. Explicit levels are validated when provider metadata is available and
serialized only by providers that support the selected level.

Bad combinations fail before a provider request when they can be known locally.
Unknown provider capability does not break `auto`.

## Users And Use Cases

- A terminal user can run latency-sensitive prompts with low or disabled
  reasoning where the selected model supports it.
- A terminal user can raise reasoning for difficult coding, planning, or
  debugging prompts without switching models.
- An ACP client can show and change the same reasoning setting for future prompt
  turns.
- A provider maintainer can add support for a new provider's reasoning contract
  without changing app, server, or tool-loop policy.
- A user inspecting logs or startup status can see which reasoning setting was
  selected without exposing secrets or provider internals.

## Public Contract

### User-Facing Values

Add one user-facing setting named `reasoning`.

Accepted values:

```text
auto
none
minimal
low
medium
high
xhigh
```

Meaning:

- `auto`: do not send an explicit provider reasoning override. Use the selected
  model and provider default.
- `none`: request no reasoning when the provider/model supports disabling it.
- `minimal`, `low`, `medium`, `high`, `xhigh`: request increasing provider
  reasoning effort when supported.

`auto` is the default everywhere.

### CLI

Add:

```text
thndrs --reasoning <auto|none|minimal|low|medium|high|xhigh>
```

Rules:

- CLI flag overrides TOML and environment configuration.
- Invalid values are rejected by clap before the app starts.
- Startup diagnostics and provider request status may include the selected
  reasoning label.
- Existing runs with no flag keep current behavior.

### TOML Config

Add:

```toml
reasoning = "auto"
```

Rules:

- The key is allowed in global and project config.
- Config precedence matches `model` and `websearch`.
- Unknown values fail config loading with a clear error.
- Config redaction does not need to redact `reasoning`.

### Environment

Add:

```text
THNDRS_REASONING=auto
```

Accepted values match the CLI.

### TUI

First milestone:

- show the selected reasoning level in startup/provider diagnostics when
  diagnostics are enabled;
- keep prompt submission and model picker behavior unchanged.

Later milestone:

- add a focused picker or slash command for changing reasoning inside the TUI
  after the request contract has settled.

### ACP

Add a session config option:

```text
id: reasoning
label: Reasoning
category: ModelConfig
values: auto, none, minimal, low, medium, high, xhigh
```

Behavior:

- `session/new` exposes the current value.
- Config updates validate the value and persist it in ACP session metadata.
- Future prompt turns use the latest session value.
- Existing ACP clients that ignore the option continue working.

## Provider Contract

Add a provider-neutral `ReasoningLevel` type and pass it through
`AgentRunConfig`.

Provider behavior:

- `auto`: omit provider-specific reasoning fields.
- unsupported explicit level: return a local validation error when support is
  known; otherwise let the provider response produce a normal provider error.
- provider serialization is owned by provider modules, not the app loop.

### Umans

Use live `ModelInfo.capabilities.reasoning` when metadata is loaded.

Validation:

- if `supported == false`, reject all explicit levels;
- if level is `none` and `can_disable == false`, reject it;
- if `levels` is non-empty, require explicit levels to be listed there;
- if `levels` is empty and reasoning is supported, treat the model as
  provider-default only until Umans documents a request field.

Request body:

- first implementation should not invent a field for Umans.
- add request serialization only after the provider contract is confirmed.

### OpenCode Go

OpenCode Go can route to OpenAI-compatible chat completions or
Anthropic-compatible messages.

Validation:

- support must be capability-gated by documented model/provider behavior before
  sending explicit reasoning fields;
- `auto` remains allowed for every model.

Request body:

- do not add reasoning fields to chat-completions requests by default.
- add Anthropic or OpenAI-shaped reasoning fields only for known supported
  model families.

### OpenCode Zen

OpenCode Zen currently uses an OpenAI-compatible chat-completions route.

Validation and request behavior:

- `auto` is allowed.
- explicit levels should be rejected or left unsupported until the route
  documents a reasoning field.

### ChatGPT Codex

ChatGPT Codex uses a Responses-like request body but targets a ChatGPT-backed
experimental endpoint.

Validation and request behavior:

- `auto` is allowed and preserves current behavior.
- explicit levels may map to `reasoning: { "effort": "<level>" }` only after a
  smoke test confirms the backend accepts it.
- because this provider is experimental, keep errors clear and provider-scoped.

## Implementation Shape

### Types

Add a shared type, likely in `src/core/providers/mod.rs` or a small config
module:

```rust
pub enum ReasoningLevel {
    Auto,
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}
```

Required helpers:

- parse from lowercase labels;
- render stable lowercase labels;
- return whether the value is `auto`;
- return provider payload string for explicit levels.

Use `serde` and `clap::ValueEnum` where needed instead of duplicating parsing
logic.

### Config Plumbing

Extend:

- `src/core/config/mod.rs`
- `src/cli/mod.rs`
- `src/core/tools.rs`
- `src/core/agent.rs`
- `src/server/config_options.rs`
- `src/server/session.rs`
- `src/server/handlers.rs`

`AgentRunConfig::new` should preserve existing callers by defaulting reasoning
to `ReasoningLevel::Auto` or accept a new argument only where all callers can be
updated in one small pass.

### Provider Request Plumbing

Extend the provider trait request boundary to include reasoning:

```rust
fn send_streaming_request(
    &self,
    model: &str,
    messages: &[ProviderMessage],
    max_tokens: u32,
    search_mode: WebSearchMode,
    reasoning: ReasoningLevel,
    tools: &serde_json::Value,
) -> Result<Response<Body>>;
```

If passing one more argument makes call sites noisy, introduce a small
`ProviderRequestConfig` struct with `max_tokens`, `search_mode`, and
`reasoning`.

### Validation

Prefer provider-local validation functions:

```rust
fn validate_reasoning_level(
    model: &str,
    metadata: Option<&Metadata>,
    reasoning: ReasoningLevel,
) -> Result<(), ProviderError>;
```

Rules:

- validation should be cheap and pure;
- `auto` always succeeds;
- explicit unsupported values produce actionable messages;
- missing metadata should not block providers that have static capability
  rules.

### Request Body Builders

Update body builders only where the wire shape is known:

- OpenAI Responses-like builders may add `reasoning.effort` for explicit levels.
- Anthropic-compatible builders must use provider-specific `thinking` fields
  only when the target provider/model supports them.
- OpenAI chat-completions builders should not add Responses-only fields.

## Testing Plan

Unit tests:

- `ReasoningLevel` parses and renders all accepted labels.
- invalid TOML and env values fail clearly.
- config precedence works: defaults < global < project < env < CLI.
- `AgentRunConfig` defaults to `auto`.
- ACP config option exposes all values and validates updates.
- Umans metadata validation rejects unsupported levels.
- provider request builders omit reasoning for `auto`.
- provider request builders include the expected field only when support is
  enabled and explicit.

Integration or smoke tests:

- fake provider runs still work with default `auto`.
- ACP prompt turns preserve session reasoning config.
- provider request status includes model, websearch, and reasoning without
  leaking credentials.

Manual review:

- check provider error messages for unsupported reasoning values;
- verify no provider receives a reasoning field for `auto`;
- verify existing model picker and first-run setup flows are unchanged.

## Commands

For Rust changes:

```text
cargo fmt
cargo clippy --fix --allow-dirty --allow-staged
cargo clippy
cargo test
```

For public docs changes outside `docs/internal`:

```text
pnpm --dir docs build
```

This feature plan only adds internal docs. No docs build is required for the
planning artifact itself.

## Boundaries

Always:

- preserve current behavior for users who do not set `reasoning`;
- default to `auto`;
- keep provider-specific wire details inside provider modules;
- add tests before enabling provider-specific request serialization.

Ask first:

- adding dependencies;
- changing model ids to encode reasoning levels;
- enabling explicit reasoning for experimental ChatGPT Codex before a smoke
  test;
- changing public docs outside `docs/internal`;
- adding TUI interaction surfaces beyond diagnostics and ACP config plumbing.

Never:

- put reasoning defaults into secrets or credential stores;
- silently downgrade an explicit unsupported level to `auto`;
- send Responses-only fields to chat-completions routes without provider
  support;
- expose raw hidden chain-of-thought in new UI.

## Deferred Milestones

- TUI slash command or picker for changing reasoning interactively.
- Provider capability discovery for OpenCode model metadata if the API exposes
  reasoning support later.
- More granular provider budgets, such as Anthropic `budget_tokens`, if a
  concrete provider contract requires it.
- Per-task adaptive defaults, such as using lower reasoning for summarization
  and higher reasoning for code changes.
- Session export and inspect enhancements that show reasoning-level history per
  prompt turn.

## Risks And Open Questions

- Provider names and accepted levels are model-dependent and may change.
- OpenAI Responses reasoning fields do not automatically apply to
  OpenAI-compatible chat-completions routes.
- Anthropic-compatible providers differ across model families, so a single
  effort enum may not cover token-budget controls.
- Umans metadata exposes reasoning capability, but the request field for
  controlling it still needs confirmation.
- ChatGPT Codex is experimental and may reject public API-shaped fields even
  when the body resembles Responses.
