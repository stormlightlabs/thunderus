# ACP Agent Server Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-04

## Source Notes

This plan is based on the local
[`acp.md`](../../../src/content/docs/notebook/acp.md) notebook, ACP
[architecture](https://agentclientprotocol.com/get-started/architecture), ACP
[agents](https://agentclientprotocol.com/get-started/agents), ACP
[clients](https://agentclientprotocol.com/get-started/clients), ACP
[initialization](https://agentclientprotocol.com/protocol/v1/initialization),
ACP [session setup](https://agentclientprotocol.com/protocol/v1/session-setup),
ACP [prompt turns](https://agentclientprotocol.com/protocol/v1/prompt-turn),
ACP [tool calls](https://agentclientprotocol.com/protocol/v1/tool-calls), ACP
[cancellation](https://agentclientprotocol.com/protocol/v1/cancellation), ACP
[session config options](https://agentclientprotocol.com/protocol/v1/session-config-options),
ACP [transports](https://agentclientprotocol.com/protocol/v1/transports), the
official [Rust library](https://agentclientprotocol.com/libraries/rust), and
the [`agent-client-protocol 1.0.1` docs.rs API](https://docs.rs/agent-client-protocol/1.0.1/agent_client_protocol/).

Local implementation context comes from the ACP client
[`008_acp`](../008_acp/plan.md), sessions
[`005_sessions`](../005_sessions/plan.md), tool registry
[`006_tool_registry`](../006_tool_registry/plan.md), configuration
[`003_configuration`](../003_configuration/plan.md), and MCP
[`007_mcp`](../007_mcp/plan.md) feature plans.

## Objective

Expose the built-in `thndrs` coding harness as an ACP agent that editor and IDE
clients can launch over stdio.

The desired shape is:

```text
VS Code / Neovim / Zed / other ACP client
  -> thndrs-acp-agent
  -> thndrs prompt assembly, providers, tools, sessions, context, skills
```

This is more strategically important than using `thndrs` only as an ACP client,
because it lets editors drive the `thndrs` harness without bespoke editor
plugins.

## Users And Use Cases

- A VS Code, Neovim, or Zed user can configure `thndrs-acp-agent` as an ACP
  agent and use the editor's ACP UI to prompt the `thndrs` harness.
- A contributor can test the harness through ACP fixtures without opening the
  TUI.
- A future editor extension can rely on ACP instead of a custom `thndrs`
  protocol.
- A user can keep `thndrs` sessions, config provenance, AGENTS.md loading,
  skills, tool policy, and provider behavior while changing only the front end.

## Current State

- `thndrs` has one main binary in `src/main.rs` that starts the TUI.
- `src/app.rs` owns TEA app state, transcript entries, cancellation, queued
  input, and key/mouse handling.
- `src/agent.rs` owns the provider-backed run loop and emits `AgentEvent`s over
  `std::sync::mpsc`.
- `src/session/mod.rs` owns append-only JSONL session persistence.
- `src/tools.rs` owns built-in tool definitions, workspace containment, output
  caps, file-write audit, and shell execution audit.
- The provider/tool harness is tightly coupled to `AgentEvent`, but not
  inherently tied to terminal rendering.
- `008_acp` plans `thndrs` as an ACP client for external agents. This feature
  plans the inverse direction: external ACP clients drive `thndrs`.

## Feature Outcome

An ACP client can launch `thndrs-acp-agent` over stdio, initialize the
connection, create a session, send a text prompt, receive streamed ACP
`session/update` notifications for assistant text, reasoning, tool calls,
usage, failures, and cancellation, and receive a final `session/prompt`
response.

The server uses the existing `thndrs` harness and keeps local session JSONL as
the audit source. Sensitive built-in actions such as file writes and shell
commands are represented as ACP tool calls and require ACP client permission
before execution.

## Design Choice

Use a separate binary target in the same crate:

```text
src/bin/thndrs-acp-agent.rs
```

Do not split a separate crate at first. Server mode needs shared access to
configuration, prompt assembly, session persistence, tool policy, providers,
skills, and context loading. A separate crate would force a public internal API
before the harness boundary is stable.

The binary must be protocol-clean:

- stdout carries only ACP JSON-RPC messages;
- stdin carries only ACP JSON-RPC messages from the client;
- diagnostics go to stderr or tracing sinks that do not pollute stdout.

## Public Contract

### Binary

Add:

```text
thndrs-acp-agent
```

The binary starts an ACP agent over stdio and exits when stdin closes, the ACP
connection fails, or the process receives a termination signal.

It accepts ordinary config-affecting flags that are safe in a stdio agent:

```text
thndrs-acp-agent [--cwd <path>] [--model <model>] [--websearch <mode>] [--session-dir <path>] [--config <path>]
```

The exact flag set should reuse existing config resolution where possible.
TUI-only flags such as theme, mouse, alt-screen compatibility, and
print-prompt do not belong on this binary unless they affect harness behavior.

### Protocol Baseline

Implement ACP v1 over stdio:

- `initialize`
- `session/new`
- `session/prompt`
- `session/cancel`
- `session/update`

Advertise only capabilities that work:

- text prompts;
- session cancellation;
- config options for supported settings after M10;
- no terminal capability until M16;
- no rich content until M18;
- no session load/resume/list until M14.

### Prompt Turns

For each `session/prompt`:

1. Convert text `ContentBlock`s into a user turn.
2. Reject unsupported content blocks with a clear protocol error until M18.
3. Run the existing `thndrs` harness against the session workspace.
4. Stream ACP `session/update` notifications as harness events arrive.
5. Respond to `session/prompt` with the final stop reason.

### Tool Calls

Built-in tool execution remains in `thndrs`, but the ACP client must see what
is happening:

- emit ACP tool-call updates for read, search, edit, shell, and future external
  tools;
- include title, kind, status, content, raw input where safe, and locations
  where known;
- cap and redact raw input/output before sending;
- persist the same local session records currently used by the TUI.

Sensitive operations require ACP permission:

- file writes;
- shell commands;
- future external tools with side effects.

Read-only workspace-contained tools may run without permission unless future
policy changes require approval.

### Permission Flow

Before a sensitive operation, `thndrs-acp-agent` calls
`session/request_permission` on the client with the corresponding tool-call id
and options such as allow once or reject once.

If the client cancels the prompt or the permission request cannot be answered,
the operation is rejected and the turn continues or fails according to the same
tool failure rules used by the TUI.

Do not persist blanket approvals in the first server milestones. Persistent
permission policy should be planned after real editor behavior is observed.

### Sessions

`thndrs-acp-agent` creates local `thndrs` session JSONL files for ACP-driven
sessions. The local session id and ACP session id are distinct and both are
recorded.

The ACP client sees the ACP session id. Local inspect/export continues to use
`thndrs` session ids and includes ACP metadata.

## Implementation Shape

### Dependencies

Use:

```toml
agent-client-protocol = "1.0.1"
tokio = { version = "1", features = ["rt", "macros", "sync", "time"] }
```

Server mode can use Tokio in the ACP binary/runtime boundary. That keeps async
protocol handling, request cancellation, timers, and client permission requests
simple without forcing the TUI to become async.

Tokio must not leak into renderer code, config parsing, session record types,
or built-in tool implementations unless a later refactor deliberately chooses
that direction.

### Modules

Add:

- `src/bin/thndrs-acp-agent.rs`: stdio entrypoint.
- `src/acp_server/mod.rs`: module docs and public server entrypoint.
- `src/acp_server/handlers.rs`: ACP request handlers.
- `src/acp_server/session.rs`: ACP session state and id mapping.
- `src/acp_server/events.rs`: `AgentEvent` to ACP update conversion.
- `src/acp_server/permissions.rs`: permission request bridge.
- `src/acp_server/config_options.rs`: model/search config options.
- `src/acp_server/tests.rs`: protocol and conversion tests.

Shared harness extraction should prefer:

- `src/harness/mod.rs` or a similarly small module;
- no renderer dependency;
- one function that starts a turn and emits `AgentEvent`s;
- one explicit cancellation handle;
- one explicit session writer boundary.

### Runtime Boundary

The ACP server should not call `app::update` or instantiate the TUI. It should
call a shared harness API that returns the same stream of semantic events the
TUI currently consumes.

Target shape:

```rust
pub struct HarnessTurn {
    pub config: AgentRunConfig,
    pub prompt: String,
    pub session: HarnessSession,
}

pub struct HarnessHandle {
    pub events: Receiver<AgentEvent>,
    pub cancel: CancelToken,
}

pub fn spawn_harness_turn(turn: HarnessTurn) -> HarnessHandle;
```

The exact names can change, but the boundary must preserve:

- prompt input;
- provider/model/search config;
- workspace root;
- session writer;
- skill/context metadata;
- cancellation;
- event stream.

## Boundaries

Always:

- Keep stdout protocol-clean.
- Use the official ACP Rust SDK.
- Treat ACP session ids as opaque.
- Keep local `thndrs` session ids distinct from ACP session ids.
- Request client permission before file writes and shell commands.
- Preserve existing workspace containment, caps, redaction, and audit rules.
- Test event-to-ACP conversion with fixtures.

Ask first:

- Splitting a separate crate.
- Adding remote ACP transports.
- Persisting permission approvals.
- Letting client filesystem methods replace local filesystem tools.
- Enabling unstable ACP crate features.

Never:

- Write diagnostics to stdout.
- Auto-approve sensitive tool calls.
- Store raw secrets or uncapped raw tool output.
- Let editor-driven mode bypass local tool safety rules.

## Deferred Milestones

Deferred work is planned in M14-M19. These are part of the product direction,
but each depends on the baseline server being stable:

- session resume/list/delete needs stable local-to-ACP id mapping;
- client filesystem integration needs careful unsaved-buffer semantics;
- client terminal integration needs process lifecycle and display decisions;
- MCP server config depends on local MCP support;
- rich content depends on provider and prompt assembly support;
- registry packaging depends on a stable command and capability contract.

## Verification

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test acp_server`
- `cargo test agent`
- `cargo test session`
- `cargo test tools`
- `cargo test`
- `cargo run --bin thndrs-acp-agent` with a fake ACP client fixture.
- Manual smoke with at least one editor/client path once a baseline exists.

## Risks And Open Questions

- The current agent loop emits `AgentEvent`s but also assumes TUI-owned turn
  state in some places. M2 should be a real boundary cleanup, not a thin
  protocol wrapper around `app.rs`.
- ACP client permission UX varies by editor. The server should rely only on the
  protocol result, not client-specific behavior.
- Some editor clients may expect terminal support early. The baseline should
  report tool output through tool-call content first and add terminal capability
  only when the lifecycle is correct.
- The server binary needs config behavior that feels familiar but excludes
  TUI-only flags.
- `008_acp` and this feature both use ACP types. Shared conversion helpers may
  become useful, but do not create a shared abstraction until duplication is
  concrete.
