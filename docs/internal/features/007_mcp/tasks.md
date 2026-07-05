# MCP Tasks

Status: Draft
Captured: 2026-07-04

## P0: Define The Contract

- [x] Confirm `rmcp` dependency features for stdio client support.
- [x] Spike `rmcp` against a fake stdio server from a background thread.
- [x] Decide whether `rmcp` can remain behind the existing synchronous
      thread/channel boundary.
- [x] Define supported MCP protocol version behavior.
- [x] Define stdio server config schema.
- [x] Define Streamable HTTP server config schema.
- [x] Define user and project config precedence.
- [x] Define environment expansion rules.
- [x] Define server naming and tool namespacing rules.
- [x] Define startup diagnostics for disabled, skipped, failed, and ready
      servers.
- [x] Define session metadata for MCP tool calls.
- [x] MCP starts after `006_tool_registry`.
- [x] Stdio is the first implemented transport.
- [x] Streamable HTTP is part of the config contract and follows stdio.
- [x] MCP tools use the same caps, redaction, timeout, and audit path as
      built-in tools.
- [x] MCP configuration cannot rewrite built-in tool schemas or prompt identity.

## P1: Config Loading

- [x] Add `McpConfig`.
- [x] Load `~/.thndrs/mcp.toml`.
- [x] Load `.thndrs/mcp.toml`.
- [x] Merge project definitions over user definitions by server name.
- [x] Parse `transport`, `command`, `args`, `env`, `url`, `headers`,
      `timeout_secs`, and `enabled`.
- [x] Expand environment variables in values.
- [x] Skip servers with unresolved variables and record diagnostics.
- [x] Redact secret-looking config values in errors and diagnostics.
- [x] Add config tests for merge, disabled servers, unresolved env vars, and
      redaction.

## P2: SDK Adapter

- [x] Add the minimal `rmcp` dependency features needed for stdio clients.
- [x] Wrap `rmcp` client initialization behind a small `thndrs` adapter.
- [x] Convert `rmcp` initialize/server-info results to startup diagnostics.
- [x] Convert `rmcp` tool definitions to registry tool definitions.
- [x] Convert `rmcp` call results and errors to unified tool execution output.
- [x] Add adapter tests for successful initialize, tools/list, and tools/call.
- [x] Add adapter tests for malformed server messages and SDK/protocol errors.
- [x] Avoid local JSON-RPC protocol structs unless `rmcp` lacks a required
      model or stable conversion point.

## P3: Stdio Transport

- [x] Spawn stdio server processes with argv arrays through `rmcp`.
- [x] Drive initialize, tools/list, and tools/call through `rmcp`.
- [x] Capture bounded stderr diagnostics.
- [x] Enforce startup timeout.
- [x] Enforce per-call timeout.
- [x] Shut down child processes on client drop.
- [x] Add fake-server tests for initialize, tools/list, tools/call, timeout,
      stderr, and process exit.

## P4: MCP Manager

- [x] Add `McpClient` for one server.
- [x] Add `McpManager` for configured servers.
- [x] Initialize enabled servers lazily or with bounded startup.
- [x] Cache tool lists per server for prompt assembly.
- [x] Convert SDK tool input schemas to tool registry definitions.
- [x] Namespace tool names as `mcp__{server}__{tool}`.
- [x] Route namespaced calls to the correct client.
- [x] Convert SDK call results to unified tool execution output.
- [x] Add manager tests for namespace routing and duplicate tool names.

## P5: Runtime And Sessions

- [x] Include MCP tools in the provider tool catalog through the registry.
- [x] Record MCP tool start and finish records in session JSONL.
- [x] Include server name and original MCP tool name in session metadata.
- [x] Cap and redact MCP output before transcript/session storage.
- [x] Add inspect/export support for MCP calls.
- [x] Add tests proving failed MCP calls become stable tool failures.

## P6: CLI And TUI Commands

- [x] Add `thndrs mcp list`.
- [x] Add `thndrs mcp test <name>`.
- [x] Add `thndrs mcp tools <name>`.
- [x] Add `thndrs mcp call <server> <tool> --json <args>`.
- [x] Add TUI `mcp` command.
- [x] Add TUI `mcp tools <name>` command.
- [x] Add CLI parser tests.
- [x] Add command output tests with fake servers.

## P6.5: Stdio Stability Gate

Stdio has a working implementation and the targeted MCP tests pass, but it is
not stable enough to unblock HTTP transport work yet. Treat Streamable HTTP as
blocked until this gate is complete.

- [ ] Add MCP-specific provider catalog coverage proving cached stdio tools are
      included as stable `mcp__{server}__{tool}` entries in prompt/tool schema
      output.
- [ ] Add a regression test for mismatched negotiated protocol versions so the
      current diagnostic-only compatibility behavior is locked down.
- [ ] Add a regression test for bounded stderr truncation, not only stderr
      inclusion on timeout.
- [ ] Smoke-test one real stdio MCP server through `thndrs mcp test`,
      `thndrs mcp tools`, and `thndrs mcp call`, then record the exact config,
      command output shape, and any rough edges here.
- [ ] Finish stdio-facing docs: config files, stdio setup, namespacing,
      diagnostics, security limits, and CLI commands.
- [ ] Run and record the full validation set below after the docs and missing
      regression tests are complete.
- [ ] Reassess this section; only start P7 after all stdio stability items are
      done.

## P7: HTTP Transport Follow-Up

- [ ] Add HTTP transport only after P6.5 confirms stdio is stable.
- [ ] Add `rmcp` Streamable HTTP client features.
- [ ] Map URL and header config into the SDK HTTP transport.
- [ ] Verify JSON POST and optional SSE behavior through SDK-backed fixtures.
- [ ] Apply header redaction.
- [ ] Apply response-size caps.
- [ ] Add HTTP fixture tests.

## P8: Docs

- [ ] Document MCP config files and examples.
- [ ] Document that `thndrs` uses the official Rust MCP SDK for protocol and
      transport behavior while keeping local safety/audit policy.
- [ ] Document stdio server setup.
- [ ] Document tool namespacing.
- [ ] Document failure diagnostics.
- [ ] Document security limits and what MCP tools can access.
- [ ] Update CLI reference with `mcp` commands.
- [ ] Update tool docs with external tool behavior.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [x] `cargo test mcp` (passed 2026-07-04)
- [ ] `cargo test tools`
- [ ] `cargo test session`
- [ ] `cargo test cli`
- [ ] `cargo test`
