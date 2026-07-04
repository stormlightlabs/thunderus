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

Result: `tests/acp_sdk_spike.rs` proves `AcpAgent` works from a background
thread with `futures::executor::block_on` against a fake stdio ACP agent.
Tokio is not required for the client-side M1 spike.

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

- [ ] Add session record for ACP external session metadata.
- [ ] Add session record for ACP permission request/outcome.
- [ ] Extend inspect/export to include ACP metadata.
- [ ] Persist local `thndrs` session id and ACP session id separately.
- [ ] Persist agent name and redacted command display.
- [ ] Persist selected protocol version and agent info.
- [ ] Persist assistant/reasoning/tool/usage records through existing records
      where possible.
- [ ] Ensure raw stdio lines are not persisted.
- [ ] Add session serialization/deserialization tests.
- [ ] Add inspect/export tests for ACP records.

## M10: CLI Commands

- [ ] Add `thndrs acp list`.
- [ ] Add `thndrs acp inspect <name>`.
- [ ] Add `thndrs acp smoke <name> --prompt <text>`.
- [ ] `acp list` shows enabled/disabled configured agents.
- [ ] `acp inspect` shows redacted command, args, env keys, timeout, and source.
- [ ] `acp smoke` initializes, creates a temporary session, sends one prompt,
      streams status/text, and exits.
- [ ] Add CLI parser tests.
- [ ] Add command output tests with a fake ACP agent.

## M11: Manual Fake Agent

- [ ] Add a test-only fake ACP agent binary or fixture process.
- [ ] Support scripted initialize/session/prompt behavior.
- [ ] Support scripted permission request.
- [ ] Support scripted filesystem read request.
- [ ] Support scripted filesystem write request.
- [ ] Support scripted malformed/unknown update.
- [ ] Support scripted timeout/no-response.
- [ ] Use the fake agent in integration tests instead of relying on network
      package managers such as `npx`.

## M12: Docs

- [ ] Document ACP config examples.
- [ ] Document `--model acp:<name>`.
- [ ] Document ACP permission prompts.
- [ ] Document supported and unsupported ACP capabilities.
- [ ] Document `thndrs acp list`.
- [ ] Document `thndrs acp inspect <name>`.
- [ ] Document `thndrs acp smoke <name> --prompt <text>`.
- [ ] Add troubleshooting for auth-required agents, missing commands, protocol
      stdout pollution, and unsupported terminal requests.

## M13: Auth

- [ ] Design where ACP auth state is stored.
- [ ] Redact auth state in logs, sessions, diagnostics, and inspect/export.
- [ ] Implement `authenticate` for advertised auth methods that fit local CLI
      use.
- [ ] Implement `logout` when the agent advertises logout support.
- [ ] Add recovery behavior for expired, rejected, or missing credentials.
- [ ] Add tests for auth-required startup, auth failure, auth success, and
      logout.

## M14: Terminal Callbacks

- [ ] Decide when `thndrs` advertises terminal capability.
- [ ] Implement `terminal/create` with argv arrays and workspace cwd policy.
- [ ] Implement `terminal/output` with byte caps and incremental display.
- [ ] Implement `terminal/wait_for_exit`.
- [ ] Implement `terminal/kill`.
- [ ] Implement `terminal/release`.
- [ ] Record terminal lifecycle metadata in session JSONL.
- [ ] Add UI tests and process lifecycle tests.

## M15: Agent-Owned Sessions

- [ ] Implement `session/list` when an agent advertises support.
- [ ] Implement `session/load` with replay through `session/update`.
- [ ] Implement `session/resume` when an agent advertises support.
- [ ] Implement `session/close` when an agent advertises support.
- [ ] Keep local `thndrs` session ids distinct from external ACP session ids.
- [ ] Add inspect/export support for external session metadata.
- [ ] Add tests for unsupported, supported, failed, and replayed sessions.

## M16: ACP Registry

- [ ] Decide whether registry discovery belongs in core `thndrs` or docs only.
- [ ] Fetch or read registry metadata from a stable source.
- [ ] Show available agents without installing them automatically.
- [ ] Add install/update only after command provenance and security review.
- [ ] Record installed agent source/version metadata.
- [ ] Add tests for registry parse, display, redaction, and failure behavior.

## M17: MCP-Over-ACP

- [ ] Wait for local MCP support from `007_mcp`.
- [ ] Decide whether ACP sessions receive user MCP servers, project MCP
      servers, or both.
- [ ] Pass MCP server config through `session/new` when supported.
- [ ] Support thndrs-provided MCP self-proxy only after a separate design.
- [ ] Add tests for no MCP support, stdio MCP config, and redacted diagnostics.

## M18: Remote And Custom Transports

- [ ] Re-check ACP transport docs before implementation.
- [ ] Add Streamable HTTP only after the spec is no longer draft or a target
      agent requires it.
- [ ] Add WebSocket/custom bridge support only for a concrete target client or
      deployment.
- [ ] Preserve the same JSON-RPC lifecycle, capability, timeout, redaction, and
      session-audit behavior as stdio.
- [ ] Add transport fixture tests before enabling user config.

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

- [ ] After M1, decide whether `futures::executor::block_on` is enough.
- [ ] After M2, review config shape before it becomes documented.
- [ ] After M6, review permission UX before wiring real write approvals.
- [ ] After M7, review filesystem callback policy against built-in tool policy.
- [ ] Before release, smoke test at least one real ACP agent manually.
