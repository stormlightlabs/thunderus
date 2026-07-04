# ACP Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-04

## Source Notes

This plan is based on the local
[`acp.md`](../../../src/content/docs/notebook/acp.md) notebook, the ACP
[get-started](https://agentclientprotocol.com/get-started/) docs, ACP
[architecture](https://agentclientprotocol.com/get-started/architecture), ACP
[transports](https://agentclientprotocol.com/protocol/v1/transports), ACP
[clients](https://agentclientprotocol.com/get-started/clients), the official
[Rust library](https://agentclientprotocol.com/libraries/rust),
[`agent-client-protocol` on crates.io](https://crates.io/crates/agent-client-protocol),
the [`agent-client-protocol 1.0.1` docs.rs API](https://docs.rs/agent-client-protocol/1.0.1/agent_client_protocol/),
and the published `agent-client-protocol` crate `1.0.1` source.

Local implementation context comes from the configuration
[`003_configuration`](../003_configuration/plan.md), sessions
[`005_sessions`](../005_sessions/plan.md), tool registry
[`006_tool_registry`](../006_tool_registry/plan.md), and MCP
[`007_mcp`](../007_mcp/plan.md) feature plans.

## Background

ACP standardizes the boundary between coding-agent clients and coding-agent
implementations. The client owns UI, permission prompts, filesystem policy,
terminal access, session display, and user trust. The agent owns the model loop
and reports streamed session state through JSON-RPC.

`thndrs` is currently its own coding-agent client and model harness. It talks
directly to provider APIs, dispatches local tools, renders streaming transcript
entries, and persists append-only session JSONL.

ACP should not replace that internal harness. The first useful milestone is an
additional runtime path where `thndrs` can act as an ACP client for an external
stdio agent while preserving the same UI, cancellation, session, and safety
expectations. Later milestones should add auth, terminal support, registry
integration, remote transports if the ecosystem needs them, and eventually
`thndrs` as an ACP agent if that becomes a product goal.

## Current State

- `src/agent.rs` runs provider-backed turns on a background thread and emits
  `AgentEvent`s over `std::sync::mpsc`.
- `src/app.rs` owns TEA state, transcript entries, cancellation, queued input,
  and prompt submission.
- `src/session/mod.rs` persists append-only JSONL records for user,
  assistant, reasoning, tool, usage, shell, file-write, failed, and cancelled
  events.
- `src/config/mod.rs` supports strict TOML config, `THNDRS_` env overrides,
  config provenance, and secret-shaped key rejection.
- `src/tools.rs` owns workspace containment, output caps, and structured file
  write/shell audit.
- There is no interactive permission request surface today.

## Crate Review

The official Rust SDK should be used instead of hand-rolled ACP JSON-RPC.

Relevant `agent-client-protocol = "1.0.1"` details:

- The public crate description is "Core protocol types and traits for the
  Agent Client Protocol".
- The crate exposes `Client`, `Agent`, `AcpAgent`, `ConnectionTo`,
  `ActiveSession`, `SessionMessage`, `Stdio`, typed JSON-RPC traits, and a flat
  `schema` module.
- `AcpAgent` launches a stdio external agent and implements `ConnectTo<Client>`.
  It supports command-string parsing, JSON stdio config parsing, env vars, and
  debug callbacks for stdin/stdout/stderr lines.
- `Client.builder()` can register handlers for agent-to-client notifications
  and requests, including `session/update`, `session/request_permission`,
  `fs/read_text_file`, `fs/write_text_file`, and terminal requests.
- The v1 schema includes `initialize`, `authenticate`, `session/new`,
  `session/load`, `session/list`, `session/resume`, `session/prompt`,
  `session/cancel`, `session/close`, session mode/config requests, and
  extension handling.
- `SessionBuilder` can inject MCP servers into an ACP session, but MCP over ACP
  should wait until both `007_mcp` and this feature have stable boundaries.
- The SDK is async-oriented and uses `futures`/`async-process`. The examples use
  Tokio, but the crate itself is not a Tokio-only API.
- The crate has optional unstable features for auth methods, cancellation,
  MCP-over-ACP, session fork, elicitation, and protocol v2. The first thndrs
  implementation should stay on stable v1 features unless a required target
  agent proves otherwise.

## Problem

Users cannot connect `thndrs` to external ACP-compatible coding agents such as
Codex ACP, Claude Code ACP, Gemini ACP, or a custom local ACP server.

The main risk is treating ACP like another streaming provider. ACP is not just
a model endpoint: it is a bidirectional client-agent protocol where the agent
can ask the client for permissions, filesystem reads/writes, and terminal
operations during a turn. If those requests bypass `thndrs` policy, the result
is less safe and less inspectable than the built-in harness.

## Feature Outcome

A user can configure a stdio ACP agent, select it with `--model acp:<name>`,
start `thndrs`, submit a prompt, see streamed ACP session updates in the normal
transcript, answer permission prompts in the TUI, cancel the turn, and inspect
the resulting session JSONL.

ACP filesystem requests are routed through the same workspace containment and
file-write audit expectations as built-in tools. Unsupported ACP capabilities
fail visibly and do not silently grant access.

## Goals

1. Add configured stdio ACP agents as a first-class runtime option.
2. Use the official `agent-client-protocol` Rust crate for protocol and schema
   types.
3. Keep ACP behind the existing `AgentEvent`/transcript/session path where
   possible.
4. Add a user-visible permission request flow before approving sensitive ACP
   actions.
5. Support filesystem read/write callbacks with thndrs containment and audit.
6. Support cancellation through `session/cancel` and local process cleanup.
7. Persist ACP metadata and streamed results in append-only sessions.
8. Keep terminal callbacks, remote transports, ACP auth flows, ACP registry
   install, ACP-as-agent-server, and MCP-over-ACP planned as explicit later
   milestones.

## Public Contract

### Configuration

Add ACP agent declarations to the existing TOML config contract:

```toml
[acp_agents.codex]
command = "npx"
args = ["-y", "@zed-industries/codex-acp@latest"]
env = {}
enabled = true
timeout_secs = 60
```

Rules:

- Agent names use `[A-Za-z0-9_-]+`.
- `command` is required.
- `args` defaults to `[]`.
- `env` defaults to `{}` and is redacted in diagnostics/session metadata.
- `enabled` defaults to `true`.
- `timeout_secs` defaults to `60` and applies to initialize, session creation,
  and prompt completion watchdogs.
- Project config overrides global config by ACP agent name.
- Secret-shaped TOML keys remain rejected. Explicit `env` values are allowed
  but redacted everywhere user-visible.

### Selection

Use the existing model flag for routing:

```text
thndrs --model acp:codex
```

`acp:<name>` means:

- find enabled ACP agent `<name>` in effective config;
- start a stdio ACP connection for that agent;
- use ACP for the model turn instead of the built-in provider loop.

Unknown, disabled, or malformed agents fail before the TUI starts when possible
and become a normal run failure if discovered during prompt submission.

### CLI Commands

Add read-only debug commands after the first runtime path works:

```text
thndrs acp list
thndrs acp inspect <name>
thndrs acp smoke <name> --prompt <text>
```

`acp smoke` initializes the agent, creates a temporary ACP session, sends one
prompt, prints streamed text/status rows, and exits. It is not a replacement
for the TUI.

### TUI Behavior

- ACP assistant text streams into `Entry::Agent`.
- ACP reasoning/plan text streams into `Entry::Reasoning` or `Entry::Status`
  depending on the update kind.
- ACP tool-call updates render as `Entry::Tool` with incremental status.
- Permission requests pause the turn and show a focused prompt with the
  agent-provided options.
- Escape cancels the active prompt and sends `session/cancel` when an ACP
  prompt is in flight.
- Unsupported agent requests are answered with a protocol error or cancelled
  response, and a status/error row explains what was refused.

## Implementation Shape

### Dependencies

Add:

```toml
agent-client-protocol = "1.0.1"
futures = "0.3"
```

Do not add Tokio in the first pass. Run the ACP async client inside a dedicated
background thread using `futures::executor::block_on`. If this proves
insufficient for `AcpAgent` subprocess IO, stop and record the evidence before
adding a scoped runtime dependency.

This is a deliberate milestone gate, not a permanent rejection of Tokio. The
published SDK examples use Tokio, while the crate surface is built on
`futures` and `async-process`. M1 must prove whether a small executor is enough
for stdio agents, permission callbacks, cancellation, and process cleanup. Add
Tokio only if the spike demonstrates a real runtime requirement.

### Modules

Add `src/acp/`:

- `mod.rs`: public entry points and module docs.
- `config.rs`: `AcpAgentConfig`, validation, redacted metadata.
- `runner.rs`: background ACP run loop that emits `AgentEvent`.
- `events.rs`: conversion from ACP session updates to `AgentEvent`s.
- `permissions.rs`: pending permission request representation and responses.
- `fs.rs`: ACP filesystem callback implementation.
- `tests.rs`: fake-agent integration tests and mapping tests.

Prefer small pure conversion helpers so update mapping and config behavior can
be tested without spawning subprocesses.

### Runtime Boundary

Add a branch in the existing run creation path:

- built-in model id: keep `agent::spawn_run`;
- `acp:<name>`: call `acp::spawn_run`.

`acp::spawn_run` should return the same `Receiver<AgentEvent>` shape and a
cancel handle compatible with the app's existing cancellation path. Avoid
creating a parallel transcript model.

### ACP Flow

The first implementation should:

1. Build an `AcpAgent` from validated config.
2. Register handlers for:
   - `SessionNotification`;
   - `RequestPermissionRequest`;
   - `ReadTextFileRequest`;
   - `WriteTextFileRequest`.
3. Send `InitializeRequest::new(ProtocolVersion::V1)`.
4. Reject unsupported protocol versions and unsupported required auth with a
   clear failure.
5. Create a new ACP session with the workspace root as `cwd`.
6. Send `PromptRequest` with one text `ContentBlock`.
7. Convert updates into `AgentEvent`s until `PromptResponse.stop_reason`.
8. On local cancellation, send `CancelNotification` for the ACP session and
   stop reading after the cancelled stop reason or timeout.
9. Drop the connection and ensure the child process exits or is killed.

### Permission Flow

Add a new app-level pending permission state instead of auto-approving:

```rust
pub struct PendingPermission {
    pub request_id: String,
    pub title: String,
    pub options: Vec<PermissionOption>,
}
```

The ACP handler sends an app event and waits on a one-shot response channel.
The TUI must let the user choose one of the agent-provided options or cancel.
If the run is cancelled while a permission is pending, return
`RequestPermissionOutcome::Cancelled`.

Do not persist blanket approvals in this feature.

### Filesystem Callback Policy

Support:

- `fs/read_text_file` for workspace-contained text files.
- `fs/write_text_file` for workspace-contained text writes.

Rules:

- Normalize and enforce containment before reading or writing.
- Reject directory, binary, symlink-escape, ignored-file, and oversized reads
  using existing tool limits where possible.
- Record successful and failed writes with the same audit metadata used by
  built-in write tools.
- Surface denied reads/writes as tool/error status rows and protocol errors.
- Do not support unsaved editor buffers because `thndrs` is not an editor.

### Terminal Callback Policy

Do not advertise terminal capability in milestone 1.

If an agent sends terminal requests anyway, answer with method-not-supported or
a cancelled/failed response and emit a status row. Terminal support needs a
separate plan because it touches process lifecycle, UI display, output caps,
and session audit.

### Transport Policy

ACP v1 names stdio as the concrete transport and marks Streamable HTTP as a
draft proposal. The architecture docs describe the editor booting an agent
subprocess and communicating over stdin/stdout. That means stdio is the
required first transport for ordinary editor/client interoperability.

Multiple transports are not required for baseline VS Code, Neovim, or Zed
compatibility. Those clients can launch stdio ACP agents. Remote transports are
useful later for web, mobile, multi-user, hosted, or bridge-based deployments,
but implementing them before stdio is stable would add complexity before it
unblocks the main editor use case.

### Session Persistence

Extend session records only where existing records cannot express ACP state.

Required additions:

- external ACP agent metadata: agent name, command display, selected ACP
  protocol version, agent info, and ACP session id;
- permission request/response records;
- ACP tool-call metadata when ACP update fields do not map cleanly onto
  existing `tool_started`/`tool_finished` records.

Existing records should still capture:

- user prompt;
- assistant final text;
- reasoning final text when available;
- usage increments when ACP reports them;
- tool start/finish display;
- file-write audit;
- failed/cancelled turn outcome.

Never persist raw stdio lines, raw secrets, or uncapped raw tool payloads.

## Decisions

- `thndrs` implements the ACP client side first.
- Stdio is the only supported transport in the first milestone.
- The official Rust SDK is used for protocol/schema/connection handling.
- ACP routing uses the existing `--model acp:<name>` flag to avoid a second
  provider selector.
- ACP does not go through `StreamingProvider`; it gets a separate runner that
  emits the existing `AgentEvent` stream.
- `thndrs` owns permission decisions, filesystem policy, and session audit.
- Terminal capability is intentionally not advertised yet.
- ACP auth methods are detected but not completed yet; authenticated agents are
  a follow-up unless an unauthenticated smoke test is impossible.
- MCP-over-ACP waits for the local MCP feature and a separate integration plan.

## Dependencies

- `003_configuration`: add nested ACP agent config with redacted provenance.
- `005_sessions`: add ACP metadata/permission records and inspect/export
  support.
- `006_tool_registry`: helpful for future ACP tool rendering, but not required
  for the first external-agent runtime.
- `007_mcp`: future MCP-over-ACP integration only; not a first-milestone
  dependency.

## Boundaries

Always:

- Keep stdout protocol-clean for any ACP child process.
- Redact `env` values in diagnostics, sessions, and debug output.
- Fail closed for unknown capabilities and unsupported agent-to-client
  requests.
- Use absolute normalized paths for filesystem callbacks.
- Keep ACP event conversion covered by fixture tests.

Ask first:

- Adding Tokio or another async runtime.
- Enabling unstable `agent-client-protocol` crate features.
- Adding terminal callback support.
- Persisting permission approvals.
- Supporting remote ACP transports.

Never:

- Auto-approve permissions.
- Let ACP filesystem callbacks escape the workspace root.
- Store raw secrets, raw stdio protocol logs, or uncapped tool output in
  sessions.
- Replace built-in provider behavior while adding ACP.

## Verification

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test acp`
- `cargo test config`
- `cargo test session`
- `cargo test app`
- `cargo test`
- Manual smoke: configure one local fake ACP agent and run
  `thndrs --model acp:<name>`.
- Manual smoke: run `thndrs acp smoke <name> --prompt "say hello"` and verify
  initialize, session creation, streamed text, and process cleanup.

## Risks And Open Questions

- The SDK examples use Tokio. The first implementation assumes the crate can be
  driven with `futures::executor::block_on`; verify this before wiring UI code.
- Permission UI is new product surface and must be small enough not to derail
  the protocol implementation.
- ACP agents may expect terminal support. The first milestone should make that
  unsupported capability explicit rather than pretending it works.
- ACP session ids are external opaque values. `thndrs` session ids remain local
  JSONL ids; both should be recorded without conflating them.
- Authenticated ACP agents need a follow-up design for secret storage,
  `authenticate`, `logout`, and failure recovery.
