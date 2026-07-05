# ACP Tasks

Status: Draft
Captured: 2026-07-04

## M0: Confirm The Contract

- [x] Confirm ACP milestone is client-side external-agent support, not
      `thndrs` as an ACP server.
- [x] Confirm stdio is the only milestone-1 ACP transport.
- [x] Confirm routing through `--model acp:<name>`.
- [x] Confirm config shape for `[acp_agents.<name>]`.
- [x] Confirm auth-required agents fail clearly in milestone 1.
- [x] Confirm terminal capability is not advertised in milestone 1.
- [x] Confirm MCP-over-ACP is out of scope until after `007_mcp`.
- [x] Confirm no unstable `agent-client-protocol` features are enabled.
- [x] Confirm dependency choice:
      `agent-client-protocol = "1.0.1"` and `futures = "0.3"`.

## M1: SDK Spike

- [x] Add a temporary local spike or focused test proving
      `agent-client-protocol::AcpAgent` can be driven from a background thread
      with `futures::executor::block_on`.
- [x] Use a fake stdio ACP agent process for the spike.
- [x] Exercise `initialize`.
- [x] Exercise `session/new`.
- [x] Exercise `session/prompt`.
- [x] Receive at least one `session/update` notification.
- [x] Verify stderr capture/debug callbacks do not leak into protocol stdout.
- [x] Verify dropping the connection kills or exits the child process.
- [x] Confirm Tokio is not required for the spike; no `plan.md` runtime update
      is needed.

Result: the M1 spike proved `AcpAgent` works from a background thread with
`futures::executor::block_on` against a fake stdio ACP agent. Tokio is not
required for the client-side M1 spike. The temporary spike test was removed
after the production ACP runner covered the same runtime boundary.

## M2: Config

- [x] Add `AcpAgentConfig` and `AcpAgentsConfig`.
- [x] Parse `[acp_agents.<name>]` from TOML.
- [x] Validate agent names as `[A-Za-z0-9_-]+`.
- [x] Require `command`.
- [x] Default `args` to `[]`.
- [x] Default `env` to `{}`.
- [x] Default `enabled` to `true`.
- [x] Default `timeout_secs` to `60`.
- [x] Merge project ACP agents over global ACP agents by name.
- [x] Preserve existing unknown-key rejection.
- [x] Keep secret-shaped key rejection.
- [x] Redact ACP env values in config diagnostics and session metadata.
- [x] Add config tests for valid agent, invalid name, missing command,
      disabled agent, project override, and env redaction.

## M3: Runtime Boundary

- [x] Add `src/acp/mod.rs`.
- [x] Add `src/acp/config.rs`.
- [x] Add `src/acp/runner.rs`.
- [x] Add `src/acp/events.rs`.
- [x] Add `src/acp/permissions.rs`.
- [x] Add `src/acp/fs.rs`.
- [x] Add `src/acp/tests.rs`.
- [x] Add model-id parser for `acp:<name>`.
- [x] Route `acp:<name>` prompt submissions to `acp::spawn_run`.
- [x] Keep built-in provider models on the existing `agent::spawn_run` path.
- [x] Return the existing `Receiver<AgentEvent>` shape from ACP runs.
- [x] Preserve existing app cancellation behavior for non-ACP runs.

Result: `src/config/mod.rs` now loads, validates, merges, and redacts
`[acp_agents.<name>]`. `src/acp/runner.rs` is wired as the runtime boundary for
`--model acp:<name>` and intentionally stops before the protocol lifecycle,
which begins in M4.

## M4: ACP Connection Lifecycle

- [x] Build `agent_client_protocol::AcpAgent` from validated config.
- [x] Register `SessionNotification` handler.
- [x] Register `RequestPermissionRequest` handler.
- [x] Register `ReadTextFileRequest` handler.
- [x] Register `WriteTextFileRequest` handler.
- [x] Send `InitializeRequest::new(ProtocolVersion::V1)`.
- [x] Validate selected protocol version.
- [x] Surface `agent_info` in a status row and session metadata.
- [x] Reject unsupported required auth with a clear `AgentEvent::Failed`.
- [x] Create `session/new` with workspace root as `cwd`.
- [x] Store ACP session id as opaque external metadata.
- [x] Send `session/prompt` with text `ContentBlock`.
- [x] Convert prompt `stop_reason` into `Finished`, `Cancelled`, or `Failed`.
- [x] Ensure the child process is cleaned up on finish, failure, and drop.

Result: `src/acp/runner.rs` now drives the official SDK through initialize,
new-session, prompt, update handling, callback registration, stop-reason
conversion, and clear auth/protocol failures. ACP session ids are surfaced as
opaque status metadata; durable persistence remains in M9.

## M5: Session Update Mapping

- [x] Map ACP assistant message chunks to `AgentEvent::AssistantDelta`.
- [x] Map ACP plan/reasoning-like updates to `AgentEvent::ReasoningDelta` or
      `AgentEvent::Status`.
- [x] Map ACP usage updates to `AgentEvent::Usage`.
- [x] Map ACP tool-call start/update/completion to `ToolStarted` and
      `ToolFinished` where possible.
- [x] Preserve ACP tool call ids for correlation.
- [x] Cap and redact tool raw input/output before display/session storage.
- [x] Emit stable status rows for unsupported update variants.
- [x] Add pure fixture tests for every mapped update kind.
- [x] Add regression tests for unknown/ext update variants.

Result: `src/acp/events.rs` converts stable ACP v1 updates into existing
`AgentEvent` variants and keeps unsupported metadata/update shapes visible as
stable status rows.

## M6: Permission UI

- [x] Add app state for one pending ACP permission request.
- [x] Add `AgentEvent` variant or side channel for permission requests.
- [x] Render a focused permission prompt with title and options.
- [x] Allow keyboard selection of an agent-provided option.
- [x] Allow cancellation of the permission request.
- [x] Block normal prompt submission while permission is pending.
- [x] If the run is cancelled, respond with
      `RequestPermissionOutcome::Cancelled`.
- [x] Record permission request and selected/cancelled outcome in session JSONL.
- [x] Add app update tests for select, cancel, and run-cancel cases.
- [x] Add renderer snapshot tests for the permission surface.

Result: `src/acp/permissions.rs`, `src/app.rs`, and `src/renderer/live.rs`
now support one focused ACP permission request at a time. User selection and
cancellation are returned to the ACP runner, normal prompt submission is blocked
while a permission is pending, run cancellation responds with `Cancelled`, and
permission request/outcome metadata is recorded in session JSONL.

## M7: Filesystem Callbacks

- [x] Implement `fs/read_text_file` for workspace-contained text files.
- [x] Implement `fs/write_text_file` for workspace-contained text writes.
- [x] Reuse existing path normalization/containment helpers where possible.
- [x] Reject path traversal outside the workspace.
- [x] Reject directories.
- [x] Reject symlink escapes.
- [x] Reject oversized reads using existing output limits.
- [x] Return protocol errors or failed responses for denied reads/writes.
- [x] Emit status/tool rows for filesystem requests.
- [x] Record successful writes with existing file-write audit metadata.
- [x] Record failed writes as stable failures without modifying files.
- [x] Add tests for read ok, write ok, traversal denied, symlink denied,
      oversized read denied, and binary/non-UTF-8 read denied.

Result: `src/acp/fs.rs` now implements workspace-contained ACP read/write
callbacks using the shared tool path policy. Denied requests produce failed tool
rows and JSON-RPC errors, successful writes flow through existing `file_write`
audit metadata, and callback tests cover success, traversal, symlink, oversized,
directory, and non-UTF-8 failures.

## M8: Cancellation And Timeouts

- [x] Wire Escape/local cancel to ACP prompt cancellation.
- [x] Send `session/cancel` for an active ACP session.
- [x] Cancel pending permission requests.
- [x] Apply initialize timeout.
- [x] Apply session creation timeout.
- [x] Apply prompt completion watchdog timeout.
- [x] Convert timeout to `AgentEvent::Failed` with a clear message.
- [x] Ensure child process cleanup after timeout.
- [x] Add tests for local cancel, pending permission cancel, and timeout.

Result: `src/acp/runner.rs` now drives initialize, session creation, and
prompt requests through cancellation-aware timeout guards. Local cancellation
sends `session/cancel` for active ACP prompts, pending permission callbacks
observe the shared cancel token and respond with `Cancelled`, and prompt
timeouts fail visibly while dropping the connection to clean up the child
process. Fake-agent tests cover local cancel, pending permission cancel,
initialize timeout, session creation timeout, and prompt timeout.

## M9: Session Persistence

- [x] Add session record for ACP external session metadata.
- [x] Add session record for ACP permission request/outcome.
- [x] Extend inspect/export to include ACP metadata.
- [x] Persist local `thndrs` session id and ACP session id separately.
- [x] Persist agent name and redacted command display.
- [x] Persist selected protocol version and agent info.
- [x] Persist assistant/reasoning/tool/usage records through existing records
      where possible.
- [x] Ensure raw stdio lines are not persisted.
- [x] Add session serialization/deserialization tests.
- [x] Add inspect/export tests for ACP records.

Result: `src/session/mod.rs` now persists external ACP session metadata in an
`acp_session` record while keeping the local `thndrs` session id separate from
the opaque ACP session id. The record includes agent name, redacted command
display, selected protocol version, and optional agent info. Existing
assistant, reasoning, tool, usage, file-write, and ACP permission records carry
the rest of the run without storing raw stdio protocol lines. Session and
headless ACP command-output tests cover the new metadata surface.

## M10: CLI Commands

- [x] Add `thndrs acp list`.
- [x] Add `thndrs acp inspect <name>`.
- [x] Add `thndrs acp smoke <name> --prompt <text>`.
- [x] `acp list` shows enabled/disabled configured agents.
- [x] `acp inspect` shows redacted command, args, env keys, timeout, and source.
- [x] `acp smoke` initializes, creates a temporary session, sends one prompt,
      streams status/text, and exits.
- [x] Add CLI parser tests.
- [x] Add command output tests with a fake ACP agent.

Result: `src/cli.rs` now parses the `acp` subcommands, and `src/lib.rs`
dispatches them before entering the TUI. `acp list` and `acp inspect` render
headless config diagnostics with env values omitted, and `acp smoke` reuses
the ACP runner to initialize, create a session, send a prompt, print stream
events, and exit. Parser and command-output tests cover all three commands,
including a local fake ACP agent smoke run.

## M11: Manual Fake Agent

- [x] Add a test-only fake ACP agent binary or fixture process.
- [x] Support scripted initialize/session/prompt behavior.
- [x] Support scripted permission request.
- [x] Support scripted filesystem read request.
- [x] Support scripted filesystem write request.
- [x] Support scripted malformed/unknown update.
- [x] Support scripted timeout/no-response.
- [x] Use the fake agent in integration tests instead of relying on network
      package managers such as `npx`.

Result: `tests/fixtures/fake_acp_agent.py` is now the shared manual ACP
fixture for unit and integration tests. It supports lifecycle, cancellation,
permission, filesystem read/write, unknown update, initialize timeout, session
timeout, and prompt timeout scripts. ACP runner tests and the `acp smoke`
command-output test now invoke this fixture directly instead of writing one-off
inline Python agents.

## M12: Docs

- [x] Document ACP config examples.
- [x] Document `--model acp:<name>`.
- [x] Document ACP permission prompts.
- [x] Document supported and unsupported ACP capabilities.
- [x] Document `thndrs acp list`.
- [x] Document `thndrs acp inspect <name>`.
- [x] Document `thndrs acp smoke <name> --prompt <text>`.
- [x] Add troubleshooting for auth-required agents, missing commands, protocol
      stdout pollution, and unsupported terminal requests.

Result: public docs now include a dedicated ACP usage page, reference config
examples, `--model acp:<name>` selection, permission prompt behavior,
supported and unsupported ACP capabilities, `acp list`/`inspect`/`smoke`
commands, auth/session admin commands, and troubleshooting for missing
commands, auth failures, stdout protocol pollution, timeouts, unsupported
protocol versions, and transport limitations. The sample config includes a
commented ACP agent block.

## M13: Auth

- [x] Design where ACP auth state is stored.
- [x] Redact auth state in logs, sessions, diagnostics, and inspect/export.
- [x] Implement `authenticate` for advertised auth methods that fit local CLI
      use.
- [x] Implement `logout` when the agent advertises logout support.
- [x] Add recovery behavior for expired, rejected, or missing credentials.
- [x] Add a separate OS credential-store design before supporting any
      client-owned ACP auth method.
- [x] Add tests for auth-required startup, auth failure, auth success, and
      logout.

Decision: ACP auth state is agent-owned. `thndrs` may call stable
agent-handled `authenticate` methods and report success/failure, but it does
not store credentials, tokens, cookies, or refresh state in TOML, sessions, or
diagnostics. If ACP later requires client-owned secret storage, use a separate
design with an OS credential store instead of extending session/config files.

Result: `src/acp/runner.rs` now calls stable agent-owned `authenticate` methods
before session creation and reports auth success/failure without storing
credential state. `thndrs acp logout <name>` initializes the agent and calls
`logout` only when advertised. Auth method ids/names and outcomes may appear in
status rows, but no tokens, cookies, or raw auth payloads are persisted.

## M14: Terminal Callbacks

- [x] Decide when `thndrs` advertises terminal capability.
- [x] Implement `terminal/create` with argv arrays and workspace cwd policy.
- [x] Implement `terminal/output` with byte caps and incremental display.
- [x] Implement `terminal/wait_for_exit`.
- [x] Implement `terminal/kill`.
- [x] Implement `terminal/release`.
- [x] Record terminal lifecycle metadata in session JSONL.
- [x] Advertise `clientCapabilities.terminal` only after terminal callbacks,
      UI display, cleanup, output caps, and audit tests are complete.
- [x] Add UI tests and process lifecycle tests.

Decision: keep `clientCapabilities.terminal` absent/false until all terminal
callbacks share the built-in shell safety contract: argv-only execution,
workspace-contained cwd handling, env redaction, byte-capped output,
incremental display, cancellation/kill/release cleanup, and session audit.

Result: `src/acp/terminal.rs` owns ACP terminal processes behind a registry.
The runner now advertises terminal support, handles create/output/wait/kill/
release callbacks, caps and redacts output, keeps cwd workspace-contained,
updates the normal tool row for incremental display, and records final process
metadata through the existing `shell_exec` session audit path.

## M15: Agent-Owned Sessions

- [x] Implement `session/list` when an agent advertises support.
- [x] Implement `session/load` with replay through `session/update`.
- [x] Implement `session/resume` when an agent advertises support.
- [x] Implement `session/close` when an agent advertises support.
- [x] Keep local `thndrs` session ids distinct from external ACP session ids.
- [x] Add inspect/export support for external session metadata.
- [x] Add tests for unsupported, supported, failed, and replayed sessions.

Result: `src/acp/runner.rs` now exposes capability-gated `session/list`,
`session/load`, `session/resume`, and `session/close` helpers. `thndrs acp
list-sessions`, `load-session`, `resume-session`, and `close-session` provide
headless access to agent-owned session ids; `load-session` replays
`session/update` notifications through the existing ACP event mapper. Local
append-only `thndrs` session ids remain separate from opaque external ACP
session ids in the existing `acp_session` record.

## M16: ACP Registry

- [x] Decide whether registry discovery belongs in core `thndrs` or docs only.
- [x] Fetch or read registry metadata from a stable source.
- [x] Show available agents without installing them automatically.
- [x] Design command provenance, package-manager behavior, and security review
      before registry install/update support.
- [ ] Add install/update only after command provenance and security review.
- [ ] Record installed agent source/version metadata.
- [x] Add tests for registry parse, display, redaction, and failure behavior.

Decision: read-only registry discovery belongs in core `thndrs` after the local
config/docs path is stable. Docs alone will stale quickly, but core support must
only fetch/read official registry metadata and display available agents. It must
not install or update agents until command provenance, package-manager behavior,
and security review are designed separately.

Review gate completed for design only:

- Command provenance: registry entries are candidate metadata, not execution
  permission. Future install/update may only build argv through known
  `thndrs` distribution adapters, must reject unknown executable distribution
  shapes for install/update, and must never trust registry env values.
- Package-manager behavior: future installs must pin resolved versions, use
  argv-only process creation, avoid shell interpolation, use a `thndrs`-owned
  cache/install directory by default, disable auto-update behavior when safely
  available, and require explicit confirmation before any package-manager or
  archive action.
- Security review: future binary installs require HTTPS-only official registry
  sources, bounded downloads, safe extraction, checksum/signature verification,
  secret redaction, no credential persistence, explicit user consent, and
  failed-install cleanup semantics before they can ship.
- Installed metadata: future installs must persist registry source, registry
  document version, agent id/name/version, distribution kind, resolved
  package/archive, selected platform, command preview, install directory,
  timestamp, and install/update status separately from ACP session records.

Result: `src/acp/registry.rs` parses the official read-only registry JSON from
`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` or a
local JSON file. `thndrs acp registry` lists available agents with display-safe
id, name, version, distribution labels, and homepage metadata, omits registry
env values and install commands, and prints the install/update review gate.
Install/update and installed source/version recording remain unchecked because
they are separate implementation work after these checkpoint decisions.

## M17: MCP-Over-ACP

- [x] Wait for local MCP support from `007_mcp`.
- [x] Decide whether ACP sessions receive user MCP servers, project MCP
      servers, or both.
- [x] Map the effective user-plus-project MCP config into stable ACP
      `mcpServers` entries.
- [x] Pass MCP server config through `session/new` when supported.
- [x] Support thndrs-provided MCP self-proxy only after a separate design.
- [x] Add tests for no MCP support, stdio MCP config, and redacted diagnostics.

Decision: after `007_mcp`, ACP sessions receive the effective MCP config from
both user and project scopes, using the same merge, enable/disable, redaction,
and provenance rules as local `thndrs` MCP. Initially pass only MCP server
entries that fit the stable ACP `mcpServers` shape; a thndrs-provided MCP
self-proxy remains a separate design.

Result: ACP runs now load the effective user-plus-project MCP config without
starting local MCP clients, map enabled stdio servers into stable ACP
`mcpServers`, map Streamable HTTP only when the ACP agent advertises HTTP MCP
support, and pass the mapped entries through `session/new`. Disabled and
unsupported MCP servers produce name-only diagnostics, and existing MCP loader
diagnostics are forwarded with their existing redaction behavior. The
thndrs-provided MCP self-proxy remains intentionally unimplemented pending a
separate trust-boundary design.

## M18: Remote And Custom Transports

- [x] Re-check ACP transport docs before implementation.
- [ ] Add Streamable HTTP only after the spec is no longer draft or a target
      agent requires it.
- [ ] Add WebSocket/custom bridge support only for a concrete target client or
      deployment.
- [ ] Preserve the same JSON-RPC lifecycle, capability, timeout, redaction, and
      session-audit behavior as stdio.
- [ ] Add transport fixture tests before enabling user config.

Decision: keep stdio as the only supported transport. Current ACP v1 transport
docs still make stdio the stable baseline, describe Streamable HTTP as draft,
and allow custom transports only when they preserve ACP JSON-RPC lifecycle
requirements. Do not add remote/custom transport code without a concrete target
agent or deployment.

Research update, 2026-07-05: current mainstream ACP agent setup still does not
force `thndrs` past stdio. Codex ACP, Claude Agent ACP, Gemini/Qwen-style Zed
setups, and gateway-style agents can be configured as local commands. Transport
pressure comes from daemon/bridge deployments rather than ordinary local agent
selection: Qwen Code's `qwen serve` exposes a northbound HTTP/SSE daemon and is
tracking official Streamable HTTP; ACP Remote exposes ACP over WebSocket but
also offers a local stdio facade; Aptove Bridge and AgentRQ bridge remote/mobile
surfaces to stdio ACP agents; OpenClaw targets remote gateways through
`openclaw acp --url ...` while still presenting stdio to the ACP client.

Therefore M18 should stay closed until either a user explicitly wants direct
Qwen daemon/Streamable HTTP without `qwen --acp`, direct ACP Remote WebSocket
without a local facade, or a similar hosted deployment where a local stdio
bridge is unacceptable.

## M19: ACP Agent Server

- [x] Split editor-driven harness mode into
      `docs/internal/features/009_acp_agent_server/`.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [ ] `cargo test acp`
- [ ] `cargo test config`
- [ ] `cargo test session`
- [ ] `cargo test app`
- [ ] `cargo test`
- [ ] `thndrs acp smoke <fake-agent> --prompt "hello"`
- [ ] `thndrs --model acp:<fake-agent>`

## Review Checkpoints

- [x] After M1, decide whether `futures::executor::block_on` is enough.
- [x] After M2, review config shape before it becomes documented.
- [x] After M6, review permission UX before wiring real write approvals.
- [x] After M7, review filesystem callback policy against built-in tool policy.
- [ ] Before release, smoke test at least one real ACP agent manually.

Review results:

- M1: `futures::executor::block_on` remains enough for the implemented stdio
  ACP path; no Tokio dependency is needed without new evidence.
- M2: `[acp_agents.<name>]` is accepted for docs as implemented: required
  `command`, defaulted `args`/`env`/`enabled`/`timeout_secs`, strict names,
  project-over-global merge, and redacted env values.
- M6: the permission UX is acceptable for real ACP write approvals because only
  one request can be pending, normal prompt input is blocked, cancellation is
  explicit, blanket approvals are not stored, and outcomes are audited.
- M7: filesystem callbacks match built-in tool policy closely enough to keep
  enabled: paths are workspace-contained, symlink/directory/binary/oversized
  cases fail closed, writes are audited, and denied requests are visible.
