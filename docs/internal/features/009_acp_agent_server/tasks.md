# ACP Agent Server Tasks

Status: Draft
Captured: 2026-07-04

## M0: Confirm The Contract

- [ ] Confirm `thndrs-acp-agent` is editor-driven harness mode.
- [ ] Confirm implementation lives in the main crate first, not a separate
      crate.
- [ ] Confirm `thndrs-acp-agent` is a separate binary from the TUI.
- [ ] Confirm stdio is the first transport.
- [ ] Confirm ACP v1 baseline methods: `initialize`, `session/new`,
      `session/prompt`, `session/cancel`, and `session/update`.
- [ ] Confirm file writes and shell commands require client permission.
- [ ] Confirm local `thndrs` session ids remain distinct from ACP session ids.
- [ ] Confirm Tokio is allowed inside the ACP server binary/runtime boundary.
- [ ] Confirm TUI code does not become async as part of this feature.

## M1: SDK Spike

- [ ] Add a temporary minimal `Agent.builder()` stdio server.
- [ ] Handle `InitializeRequest`.
- [ ] Handle `NewSessionRequest`.
- [ ] Handle `PromptRequest`.
- [ ] Send one `SessionNotification` during a prompt.
- [ ] Return `PromptResponse` with a stop reason.
- [ ] Run the spike under a Tokio runtime.
- [ ] Drive the spike with a fake ACP client fixture.
- [ ] Verify stdout contains only ACP JSON-RPC messages.
- [ ] Verify diagnostics go to stderr.

## M2: Harness Boundary

- [ ] Identify current dependencies between `src/app.rs` and `src/agent.rs`.
- [ ] Extract a UI-independent harness turn type.
- [ ] Extract a UI-independent harness handle with events and cancellation.
- [ ] Keep `AgentEvent` as the first shared semantic event stream.
- [ ] Preserve existing TUI behavior through the new harness boundary.
- [ ] Keep renderer dependencies out of the harness module.
- [ ] Keep config/session/tool dependencies explicit.
- [ ] Add tests proving the TUI can still run a fake provider turn.
- [ ] Add tests proving the harness can run without constructing `App`.

## M3: Binary

- [ ] Add `src/bin/thndrs-acp-agent.rs`.
- [ ] Parse safe harness/config flags.
- [ ] Reuse existing effective config loading where possible.
- [ ] Exclude TUI-only flags.
- [ ] Initialize tracing without writing to stdout.
- [ ] Start `Agent.builder().connect_to(Stdio::new())`.
- [ ] Exit cleanly when stdin closes.
- [ ] Add a smoke test that launches the binary as a subprocess.

## M4: Initialization

- [ ] Register `InitializeRequest` handler.
- [ ] Negotiate ACP protocol version 1.
- [ ] Return `agentInfo` for `thndrs`.
- [ ] Return accurate `agentCapabilities`.
- [ ] Advertise text prompt support.
- [ ] Do not advertise rich content until M18.
- [ ] Do not advertise terminal capability until M16.
- [ ] Do not advertise session load/resume/list until M14.
- [ ] Add fixture tests for supported and unsupported protocol versions.

## M5: Sessions

- [ ] Register `NewSessionRequest` handler.
- [ ] Validate and normalize `cwd`.
- [ ] Create an opaque ACP session id.
- [ ] Create or attach a local `thndrs` session writer.
- [ ] Record ACP session metadata in local session JSONL.
- [ ] Store session state in a server session map.
- [ ] Reject duplicate or invalid session operations clearly.
- [ ] Add tests for session creation and id mapping.

## M6: Prompt Turns

- [ ] Register `PromptRequest` handler.
- [ ] Validate session id.
- [ ] Convert text `ContentBlock`s into a user prompt.
- [ ] Reject unsupported content blocks with a stable protocol error.
- [ ] Start a harness turn for the session.
- [ ] Stream events while the prompt request is pending.
- [ ] Return `PromptResponse` when the harness finishes.
- [ ] Prevent concurrent prompt turns for the same ACP session.
- [ ] Add tests for prompt success, unsupported content, missing session, and
      concurrent prompt rejection.

## M7: Session Updates

- [ ] Map `AgentEvent::AssistantDelta` to agent message chunks.
- [ ] Map `AgentEvent::ReasoningDelta` to plan/reasoning/status updates.
- [ ] Map `AgentEvent::Usage` to ACP usage updates when supported by the schema.
- [ ] Map `AgentEvent::Status` to useful ACP status/plan updates.
- [ ] Map `AgentEvent::Failed` to failed prompt outcome and error updates.
- [ ] Map `AgentEvent::Cancelled` to cancelled prompt outcome.
- [ ] Add pure conversion tests for each event variant.
- [ ] Add fixture tests for notification ordering.

## M8: Tool Calls And Permissions

- [ ] Map `ToolStarted` to ACP tool-call create/update.
- [ ] Map `ToolFinished` to ACP tool-call completion or failure.
- [ ] Classify tool kinds: read, edit, search, execute, fetch, think, or other.
- [ ] Include safe raw input when useful.
- [ ] Cap and redact raw input/output.
- [ ] Include file locations when available.
- [ ] Before file writes, call `session/request_permission`.
- [ ] Before shell commands, call `session/request_permission`.
- [ ] Reject the tool operation when permission is cancelled or rejected.
- [ ] Add tests for approved write, rejected write, approved shell, rejected
      shell, and permission cancellation.

## M9: Cancellation

- [ ] Register `session/cancel` handling.
- [ ] Cancel the active harness turn for the ACP session.
- [ ] Cancel pending permission requests.
- [ ] Return a cancelled prompt response or protocol cancellation error
      according to SDK/schema behavior.
- [ ] Preserve partial session updates already sent.
- [ ] Record cancellation in local session JSONL.
- [ ] Add tests for prompt cancellation, permission cancellation, and
      cancellation after completion.

## M10: Config Options

- [ ] Decide the initial ACP config option ids.
- [ ] Expose model selection when values are discoverable.
- [ ] Expose web search mode.
- [ ] Expose reasoning/effort options only when supported by providers.
- [ ] Handle `session/set_config_option`.
- [ ] Persist selected config in local session metadata.
- [ ] Add tests for valid option changes, invalid option ids, and dependent
      option refreshes.

## M11: Session Persistence

- [ ] Add ACP server session metadata records if needed.
- [ ] Record client info from initialization.
- [ ] Record ACP session id.
- [ ] Record permission request/outcome metadata.
- [ ] Ensure existing user/assistant/reasoning/tool/usage records still write.
- [ ] Extend inspect/export to show ACP server metadata.
- [ ] Add serialization/deserialization tests.
- [ ] Add inspect/export tests.

## M12: Editor Smoke Tests

- [ ] Add a fake ACP client integration fixture.
- [ ] Smoke `thndrs-acp-agent` initialize/session/prompt with the fake client.
- [ ] Smoke permission approval with the fake client.
- [ ] Smoke cancellation with the fake client.
- [ ] Smoke malformed request handling.
- [ ] Manually test one real editor/client path once available.
- [ ] Record tested client name/version/date in the docs or release notes.

## M13: Docs

- [ ] Document `thndrs-acp-agent`.
- [ ] Document stdio setup.
- [ ] Document editor configuration examples.
- [ ] Document supported ACP capabilities.
- [ ] Document permission behavior.
- [ ] Document session id mapping.
- [ ] Document troubleshooting for stdout pollution, config failures,
      unsupported content, and permission cancellation.

## M14: Agent-Owned Resume

- [ ] Implement `session/list`.
- [ ] Implement `session/load` with replay from local session JSONL.
- [ ] Implement `session/resume` without replay when safe.
- [ ] Implement `session/close`.
- [ ] Implement `session/delete` after deletion policy is decided.
- [ ] Add replay ordering tests.
- [ ] Add compatibility tests for clients that do not call resume methods.

## M15: Client Filesystem Integration

- [ ] Detect client `fs/read_text_file` capability.
- [ ] Use client reads for editor-visible file state when appropriate.
- [ ] Decide how local writes and client writes interact.
- [ ] Detect client `fs/write_text_file` capability.
- [ ] Preserve local session write audit.
- [ ] Add tests for client fs unavailable, read success, read failure, write
      success, and write denial.

## M16: Client Terminal Integration

- [ ] Detect client terminal capability.
- [ ] Decide when shell tools should use client terminal methods.
- [ ] Map shell lifecycle to `terminal/create`, `terminal/output`,
      `terminal/wait_for_exit`, `terminal/kill`, and `terminal/release`.
- [ ] Preserve local shell audit records.
- [ ] Add tests for output caps, cancellation, failure, and release.

## M17: MCP Server Config

- [ ] Wait for local MCP support from `007_mcp`.
- [ ] Accept MCP servers in `session/new`.
- [ ] Pass compatible MCP server config into the tool registry/runtime.
- [ ] Redact MCP env values in diagnostics and sessions.
- [ ] Add tests for no MCP, stdio MCP config, unsupported MCP transport, and
      redaction.

## M18: Rich Content

- [ ] Add prompt assembly support for non-text content blocks.
- [ ] Add provider support for image content where available.
- [ ] Add resource/resource-link handling.
- [ ] Add rejection tests for unsupported content.
- [ ] Add fixture tests for supported rich content.

## M19: Registry Packaging

- [ ] Decide package metadata for ACP registry discovery.
- [ ] Document the command clients should launch.
- [ ] Document supported capabilities.
- [ ] Add version reporting that matches release metadata.
- [ ] Add smoke checks before publishing registry metadata.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [ ] `cargo test acp_server`
- [ ] `cargo test agent`
- [ ] `cargo test session`
- [ ] `cargo test tools`
- [ ] `cargo test`
- [ ] `cargo run --bin thndrs-acp-agent`

## Review Checkpoints

- [ ] After M1, review SDK ergonomics and runtime assumptions.
- [ ] After M2, review the harness boundary before adding protocol handlers.
- [ ] After M5, review ACP/local session id mapping.
- [ ] After M8, review permission UX with at least one client fixture.
- [ ] After M12, review real editor compatibility before registry packaging.
