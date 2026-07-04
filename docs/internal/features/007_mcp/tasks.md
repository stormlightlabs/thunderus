# MCP Tasks

Status: Draft
Captured: 2026-07-04

## P0: Define The Contract

- [ ] Define supported MCP protocol version behavior.
- [ ] Define stdio server config schema.
- [ ] Define Streamable HTTP server config schema.
- [ ] Define user and project config precedence.
- [ ] Define environment expansion rules.
- [ ] Define server naming and tool namespacing rules.
- [ ] Define startup diagnostics for disabled, skipped, failed, and ready
      servers.
- [ ] Define session metadata for MCP tool calls.
- [x] MCP starts after `007_tool_registry`.
- [x] Stdio is the first implemented transport.
- [x] Streamable HTTP is part of the config contract and follows stdio.
- [x] MCP tools use the same caps, redaction, timeout, and audit path as
      built-in tools.
- [x] MCP configuration cannot rewrite built-in tool schemas or prompt identity.

## P1: Config Loading

- [ ] Add `McpConfig`.
- [ ] Load `~/.thndrs/mcp.toml`.
- [ ] Load `.thndrs/mcp.toml`.
- [ ] Merge project definitions over user definitions by server name.
- [ ] Parse `transport`, `command`, `args`, `env`, `url`, `headers`,
      `timeout_secs`, and `enabled`.
- [ ] Expand environment variables in values.
- [ ] Skip servers with unresolved variables and record diagnostics.
- [ ] Redact secret-looking config values in errors and diagnostics.
- [ ] Add config tests for merge, disabled servers, unresolved env vars, and
      redaction.

## P2: Protocol Types

- [ ] Add JSON-RPC request, response, error, and notification types.
- [ ] Add MCP initialize request/response types.
- [ ] Add MCP tool-list response types.
- [ ] Add MCP tool-call request/response types.
- [ ] Add serde tests for protocol fixtures.
- [ ] Add error tests for invalid protocol messages.

## P3: Stdio Transport

- [ ] Spawn stdio server processes with argv arrays.
- [ ] Write JSON-RPC requests to stdin.
- [ ] Read JSON-RPC responses from stdout.
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
- [ ] Convert MCP input schemas to tool registry definitions.
- [ ] Namespace tool names as `mcp__{server}__{tool}`.
- [ ] Route namespaced calls to the correct client.
- [ ] Convert MCP call results to unified tool execution output.
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
- [ ] Implement JSON POST request/response.
- [ ] Implement SSE response handling only if required by target servers.
- [ ] Apply header redaction.
- [ ] Apply response-size caps.
- [ ] Add HTTP fixture tests.

## P8: Docs

- [ ] Document MCP config files and examples.
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
