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

- [ ] Spawn stdio server processes with argv arrays through `rmcp`.
- [ ] Drive initialize, tools/list, and tools/call through `rmcp`.
- [ ] Capture bounded stderr diagnostics.
- [ ] Enforce startup timeout.
- [ ] Enforce per-call timeout.
- [ ] Shut down child processes on client drop.
- [ ] Add fake-server tests for initialize, tools/list, tools/call, timeout,
      stderr, and process exit.

## P4: MCP Manager

- [ ] Add `McpClient` for one server.
- [ ] Add `McpManager` for configured servers.
- [ ] Initialize enabled servers lazily or with bounded startup.
- [ ] Cache tool lists per server for prompt assembly.
- [ ] Convert SDK tool input schemas to tool registry definitions.
- [ ] Namespace tool names as `mcp__{server}__{tool}`.
- [ ] Route namespaced calls to the correct client.
- [ ] Convert SDK call results to unified tool execution output.
- [ ] Add manager tests for namespace routing and duplicate tool names.

## P5: Runtime And Sessions

- [ ] Include MCP tools in the provider tool catalog through the registry.
- [ ] Record MCP tool start and finish records in session JSONL.
- [ ] Include server name and original MCP tool name in session metadata.
- [ ] Cap and redact MCP output before transcript/session storage.
- [ ] Add inspect/export support for MCP calls.
- [ ] Add tests proving failed MCP calls become stable tool failures.

## P6: CLI And TUI Commands

- [ ] Add `thndrs mcp list`.
- [ ] Add `thndrs mcp test <name>`.
- [ ] Add `thndrs mcp tools <name>`.
- [ ] Add `thndrs mcp call <server> <tool> --json <args>`.
- [ ] Add TUI `mcp` command.
- [ ] Add TUI `mcp tools <name>` command.
- [ ] Add CLI parser tests.
- [ ] Add command output tests with fake servers.

## P7: HTTP Transport Follow-Up

- [ ] Add HTTP transport only after stdio is stable.
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
- [ ] `cargo test mcp`
- [ ] `cargo test tools`
- [ ] `cargo test session`
- [ ] `cargo test cli`
- [ ] `cargo test`
