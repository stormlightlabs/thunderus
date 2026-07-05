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

### Auth State Policy

ACP auth state remains agent-owned unless a later ACP version or target agent
requires client-owned secrets.

For the stable `agent` auth method, `thndrs` may call `authenticate` with the
advertised method id and let the external agent run its own login flow. Any
tokens, cookies, refresh state, or account-specific credentials must be stored
by that agent using its own CLI/keychain behavior, not by `thndrs`.

`thndrs` may persist non-secret auth metadata only when it is useful for audit
or troubleshooting, such as the ACP agent name, advertised method id/name, and
success/failure status. Session JSONL, config diagnostics, inspect/export, and
logs must never contain credential material or raw auth payloads.

If a future ACP auth method requires `thndrs` to own credentials directly, add
a separate design before implementation. That design should use an OS
credential store or similarly explicit secret backend; TOML config and session
JSONL are not acceptable secret stores.

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

After the M11 fake-agent milestone, the decision remains: advertise
`clientCapabilities.terminal` only after every terminal callback satisfies the
same safety shape as the built-in shell path. That means argv-only execution,
workspace-contained absolute cwd handling, explicit env redaction, capped
incremental output, reliable wait/kill/release lifecycle cleanup, visible tool
rows, and append-only session audit records. Until those pieces and tests exist,
terminal capability stays absent/false even if target ACP agents can use it.

### Transport Policy

As of 2026-07-04, ACP v1 names stdio as the concrete transport and marks
Streamable HTTP as a draft proposal. The architecture docs describe the editor
booting an agent subprocess and communicating over stdin/stdout. That means
stdio is the required first transport for ordinary editor/client
interoperability.

Multiple transports are not required for baseline VS Code, Neovim, or Zed
compatibility. Those clients can launch stdio ACP agents. Remote transports are
useful later for web, mobile, multi-user, hosted, or bridge-based deployments,
but implementing them before stdio is stable would add complexity before it
unblocks the main editor use case.

Remote or custom transports remain out of scope until either Streamable HTTP is
no longer draft or a concrete target client/deployment requires a bridge. Any
new transport must preserve the same JSON-RPC lifecycle, capability checks,
timeouts, redaction, process/session cleanup, and fixture coverage as stdio
before it becomes configurable.

### Registry Discovery Policy

Read-only ACP registry discovery belongs in core `thndrs` after the local
config and docs path is stable. The registry changes over time, so documentation
alone will stale quickly, while a read-only command can show users available
agents without changing their system.

The first core registry milestone should only fetch or read official registry
metadata from a stable source, parse it, and display agent names, descriptions,
versions, and distribution hints with redaction where needed. It must not
install, update, or execute registry-provided commands automatically.

Install/update support requires a later security review covering command
provenance, package-manager behavior, source/version metadata, cache policy,
and failure modes.

#### Registry Install/Update Review Checkpoints

Command provenance:

- Only the official registry CDN URL and user-provided local files are accepted
  registry sources.
- Registry entries identify candidate packages and archives; they do not grant
  permission to execute commands.
- Future install support may derive an argv list only from a known distribution
  adapter (`npx`, `uvx`, or `binary`) implemented in `thndrs`, not from an
  arbitrary registry shell string.
- Registry `args` may be displayed for review and later copied into config, but
  must not be executed until the user has explicitly selected the agent,
  distribution, and resolved version.
- Registry `env` keys may be displayed as names only. Registry env values are
  never trusted, persisted, printed, or passed through.
- Unknown distribution kinds, unknown fields that affect execution, or multiple
  conflicting install candidates must fail closed for install/update while
  remaining displayable in read-only discovery.

Package-manager behavior:

- Package-manager installs must be deterministic enough to audit: pin the
  registry-provided package/version, do not use floating `latest`, and do not
  let the package manager rewrite the selected version during install.
- Run package managers with argv-only process creation, no shell interpolation,
  workspace-contained working directories, bounded output, timeout/cancel
  handling, and redacted diagnostics.
- Do not run agent login, initialization, smoke tests, post-install prompts, or
  generated config automatically after package installation.
- Disable known package-manager auto-update behavior where the registry
  provides a safe env key for that purpose; otherwise show a warning and require
  explicit user confirmation before using that distribution.
- Use a `thndrs`-owned cache/install directory, not the workspace tree, unless
  the user explicitly chooses a destination.
- Updates must present the old source/version and new source/version before any
  package-manager or archive action runs.

Security review:

- Binary archives require archive URL allow-listing from the official registry,
  HTTPS-only downloads, bounded size, safe extraction that rejects traversal and
  symlink escapes, and checksum/signature verification before they can be
  enabled.
- No registry-provided credential, token, cookie, auth header, or secret-shaped
  env key may enter config, session metadata, logs, or child-process env.
- Every install/update must require an interactive confirmation that shows
  registry id, name, version, distribution type, package or archive source,
  resolved command preview, install directory, and env key names.
- Failed installs leave either no installed record or a record explicitly marked
  failed/incomplete; partial files must not be silently used.
- Install/update logs must keep stdout protocol-clean for any ACP process and
  use the same redaction rules as shell and ACP terminal callbacks.

Installed metadata contract:

- Persist the registry source URL or local-file label, registry document
  version, agent id, agent name, agent version, distribution kind, resolved
  package/archive, selected platform for binaries, installed command preview,
  install directory, timestamp, and install/update status.
- Store local `thndrs` install metadata separately from ACP session metadata.
  ACP session records may reference an installed agent id/version, but they must
  not duplicate credentials or raw registry payloads.
- Future uninstall/update commands must use this metadata as the source of truth
  rather than re-discovering files heuristically.

### MCP-Over-ACP Policy

MCP-over-ACP waits for local MCP support from `007_mcp`. Once local MCP config
exists, ACP sessions should receive the effective MCP server set from both user
and project scopes, using the same merge, enable/disable, provenance, timeout,
and redaction behavior as local `thndrs` MCP. Project entries override user
entries by server name, matching the rest of project-local configuration.

Initially pass only MCP server entries that fit the stable ACP `mcpServers`
shape. Do not invent an ACP-specific MCP config format and do not let an ACP
agent choose arbitrary MCP servers outside the effective `thndrs` config. A
thndrs-provided MCP self-proxy remains a separate design because it changes the
trust boundary from "agent connects to configured servers" to "agent calls back
into thndrs tools."

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
- ACP auth state is agent-owned; `thndrs` does not store ACP credentials,
  tokens, cookies, or refresh state.
- Terminal capability is advertised only after terminal callbacks meet built-in
  shell policy, output caps, cleanup, UI, and audit requirements.
- Read-only ACP registry discovery belongs in core `thndrs`; install/update is
  a separate security-reviewed feature.
- MCP-over-ACP waits for the local MCP feature, then passes the effective
  user-plus-project MCP server config when it fits stable ACP `mcpServers`.
- Remote/custom transports wait for a stable Streamable HTTP spec or a concrete
  target deployment.

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
