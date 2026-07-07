# ACP Agent Server Tasks

Status: Draft
Captured: 2026-07-04

## M0: Confirm The Contract

- [x] Confirm `thndrs-acp-server` is editor-driven harness mode.
- [x] Confirm implementation lives in the main crate first, not a separate
      crate.
- [x] Confirm `thndrs-acp-server` is a separate binary from the TUI.
- [x] Confirm stdio is the first transport.
- [x] Confirm ACP v1 baseline methods: `initialize`, `session/new`,
      `session/prompt`, `session/cancel`, and `session/update`.
- [x] Confirm file writes and shell commands require client permission.
- [x] Confirm local `thndrs` session ids remain distinct from ACP session ids.
- [x] Confirm Tokio is allowed inside the ACP server binary/runtime boundary.
- [x] Confirm TUI code does not become async as part of this feature.

## M1: SDK Spike

- [x] Add a temporary minimal `Agent.builder()` stdio server.
- [x] Handle `InitializeRequest`.
- [x] Handle `NewSessionRequest`.
- [x] Handle `PromptRequest`.
- [x] Send one `SessionNotification` during a prompt.
- [x] Return `PromptResponse` with a stop reason.
- [x] Run the spike under a Tokio runtime.
- [x] Drive the spike with a fake ACP client fixture.
- [x] Verify stdout contains only ACP JSON-RPC messages.
- [x] Verify diagnostics go to stderr.

## M2: Harness Boundary

- [x] Identify current dependencies between `src/cli/app.rs` and `src/core/agent.rs`.
- [x] Extract a UI-independent harness turn type.
- [x] Extract a UI-independent harness handle with events and cancellation.
- [x] Keep `AgentEvent` as the first shared semantic event stream.
- [x] Preserve existing TUI behavior through the new harness boundary.
- [x] Keep renderer dependencies out of the harness module.
- [x] Keep config/session/tool dependencies explicit.
- [x] Add tests proving the TUI can still run a fake provider turn.
- [x] Add tests proving the harness can run without constructing `App`.

## M3: Binary

- [x] Add `src/bin/thndrs-acp-server.rs`.
- [x] Parse safe harness/config flags.
- [x] Reuse existing effective config loading where possible.
- [x] Exclude TUI-only flags.
- [x] Initialize tracing without writing to stdout.
- [x] Start `Agent.builder().connect_to(...)` with a stdio transport.
- [x] Exit cleanly when stdin closes.
- [x] Add a smoke test that launches the binary as a subprocess.

## M4: Initialization

- [x] Register `InitializeRequest` handler.
- [x] Negotiate ACP protocol version 1.
- [x] Return `agentInfo` for `thndrs`.
- [x] Return accurate `agentCapabilities`.
- [x] Advertise text prompt support.
- [x] Do not advertise rich content until M18.
- [x] Do not advertise terminal capability until M16.
- [x] Do not advertise session load/resume/list until M14.
- [x] Add fixture tests for supported and unsupported protocol versions.

## M5: Sessions

- [x] Register `NewSessionRequest` handler.
- [x] Validate and normalize `cwd`.
- [x] Create an opaque ACP session id.
- [x] Create or attach a local `thndrs` session writer.
- [x] Record ACP session metadata in local session JSONL.
- [x] Store session state in a server session map.
- [x] Reject duplicate or invalid session operations clearly.
- [x] Add tests for session creation and id mapping.

## M6: Prompt Turns

- [x] Register `PromptRequest` handler.
- [x] Validate session id.
- [x] Convert text `ContentBlock`s into a user prompt.
- [x] Reject unsupported content blocks with a stable protocol error.
- [x] Start a harness turn for the session.
- [x] Stream events while the prompt request is pending.
- [x] Return `PromptResponse` when the harness finishes.
- [x] Prevent concurrent prompt turns for the same ACP session.
- [x] Add tests for prompt success, unsupported content, missing session, and
      concurrent prompt rejection.

## M7: Session Updates

- [x] Map `AgentEvent::AssistantDelta` to agent message chunks.
- [x] Map `AgentEvent::ReasoningDelta` to plan/reasoning/status updates.
- [x] Map `AgentEvent::Usage` to ACP usage updates when supported by the schema.
- [x] Map `AgentEvent::Status` to useful ACP status/plan updates.
- [x] Map `AgentEvent::Failed` to failed prompt outcome and error updates.
- [x] Map `AgentEvent::Cancelled` to cancelled prompt outcome.
- [x] Add pure conversion tests for each event variant.
- [x] Add fixture tests for notification ordering.

## M8: Tool Calls And Permissions

- [x] Map `ToolStarted` to ACP tool-call create/update.
- [x] Map `ToolFinished` to ACP tool-call completion or failure.
- [x] Classify tool kinds: read, edit, search, execute, fetch, think, or other.
- [x] Include safe raw input when useful.
- [x] Cap and redact raw input/output.
- [x] Include file locations when available.
- [x] Before file writes, call `session/request_permission`.
- [x] Before shell commands, call `session/request_permission`.
- [x] Reject the tool operation when permission is cancelled or rejected.
- [x] Add tests for approved write, rejected write, approved shell, rejected
      shell, and permission cancellation.

## M9: Cancellation

- [x] Register `session/cancel` handling.
- [x] Cancel the active harness turn for the ACP session.
- [x] Cancel pending permission requests.
- [x] Return a cancelled prompt response or protocol cancellation error
      according to SDK/schema behavior.
- [x] Preserve partial session updates already sent.
- [x] Record cancellation in local session JSONL.
- [x] Add tests for prompt cancellation, permission cancellation, and
      cancellation after completion.

## M10: Config Options

- [x] Decide the initial ACP config option ids.
- [x] Expose model selection when values are discoverable.
- [x] Expose web search mode.
- [x] Expose reasoning/effort options only when supported by providers.
- [x] Handle `session/set_config_option`.
- [x] Persist selected config in local session metadata.
- [x] Add tests for valid option changes, invalid option ids, and dependent
      option refreshes.

## M11: Session Persistence

- [x] Add ACP server session metadata records if needed.
- [x] Record client info from initialization.
- [x] Record ACP session id.
- [x] Record permission request/outcome metadata.
- [x] Ensure existing user/assistant/reasoning/tool/usage records still write.
- [x] Extend inspect/export to show ACP server metadata.
- [x] Add serialization/deserialization tests.
- [x] Add inspect/export tests.

## M12: Editor Smoke Tests

- [x] Add a fake ACP client integration fixture.
- [x] Smoke `thndrs-acp-server` initialize/session/prompt with the fake client.
- [x] Smoke permission approval with the fake client.
- [x] Smoke cancellation with the fake client.
- [x] Smoke malformed request handling.
- [ ] Manually test one real editor/client path once available.
- [x] Record tested client name/version/date in the docs or release notes.

Tested client record: local fake ACP client fixture `fake-client` 0.1.0 on
2026-07-05. No real editor ACP client was available in this workspace during
M12 implementation.

## M13: Docs

- [x] Document `thndrs-acp-server`.
- [x] Document stdio setup.
- [x] Document editor configuration examples.
- [x] Document supported ACP capabilities.
- [x] Document permission behavior.
- [x] Document session id mapping.
- [x] Document troubleshooting for stdout pollution, config failures,
      unsupported content, and permission cancellation.

## M14: Agent-Owned Resume

- [x] Implement `session/list`.
- [x] Implement `session/load` with replay from local session JSONL.
- [x] Implement `session/resume` without replay when safe.
- [x] Implement `session/close`.
- [x] Implement `session/delete` after deletion policy is decided.
- [x] Add replay ordering tests.
- [x] Add compatibility tests for clients that do not call resume methods.

## M15: Client Filesystem Integration

- [x] Detect client `fs/read_text_file` capability.
- [x] Use client reads for editor-visible file state when appropriate.
- [x] Decide how local writes and client writes interact.
- [x] Detect client `fs/write_text_file` capability.
- [x] Preserve local session write audit.
- [x] Add tests for client fs unavailable, read success, read failure, write
      success, and write denial.

## M16: Client Terminal Integration

- [x] Detect client terminal capability.
- [x] Decide when shell tools should use client terminal methods.
- [x] Map shell lifecycle to `terminal/create`, `terminal/output`,
      `terminal/wait_for_exit`, `terminal/kill`, and `terminal/release`.
- [x] Preserve local shell audit records.
- [x] Add tests for output caps, cancellation, failure, and release.

## M17: MCP Server Config

- [x] Wait for local MCP support from the archived MCP feature.
- [x] Accept MCP servers in `session/new`.
- [x] Pass compatible MCP server config into the tool registry/runtime.
- [x] Redact MCP env values in diagnostics and sessions.
- [x] Add tests for no MCP, stdio MCP config, unsupported MCP transport, and
      redaction.

## M18: Rich Content

- [x] Add prompt assembly support for non-text content blocks.
- [x] Add provider support for image content where available.
- [x] Add resource/resource-link handling.
- [x] Add rejection tests for unsupported content.
- [x] Add fixture tests for supported rich content.

## M19: Registry Packaging

- [ ] Decide package metadata for ACP registry discovery.
- [ ] Document the command clients should launch.
- [ ] Document supported capabilities.
- [ ] Add version reporting that matches release metadata.
- [ ] Add smoke checks before publishing registry metadata.

## M20: Remote And Custom Transports

- [ ] Keep stdio as the only supported ACP server transport until a concrete
      editor, hosted, bridge, or daemon deployment requires more.
- [ ] Add Streamable HTTP only after the spec is no longer draft or a target
      client requires it.
- [ ] Add WebSocket/custom bridge support only for a concrete target client or
      deployment.
- [ ] Preserve the same JSON-RPC lifecycle, capability, timeout, redaction,
      cleanup, and session-audit behavior as stdio.
- [ ] Add transport fixture tests before enabling user config.

Decision carried from the archived ACP client feature: stdio remains enough for
current mainstream ACP use. Codex ACP, Claude Agent ACP, Gemini/Qwen-style Zed
setups, and gateway-style agents can be configured as local commands. Revisit
direct Streamable HTTP, WebSocket, or custom bridge support only when a local
stdio facade is not acceptable for a real target deployment.

## Review Checkpoints

- [ ] After M1, review SDK ergonomics and runtime assumptions.
- [ ] After M2, review the harness boundary before adding protocol handlers.
- [ ] After M5, review ACP/local session id mapping.
- [ ] After M8, review permission UX with at least one client fixture.
- [ ] After M12, review real editor compatibility before registry packaging.
