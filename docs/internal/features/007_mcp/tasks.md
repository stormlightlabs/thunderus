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

- [x] Add MCP-specific provider catalog coverage proving cached stdio tools are
      included as stable `mcp__{server}__{tool}` entries in prompt/tool schema
      output.
- [x] Add a regression test for mismatched negotiated protocol versions so the
      current diagnostic-only compatibility behavior is locked down.
- [x] Add a regression test for bounded stderr truncation, not only stderr
      inclusion on timeout.
- [x] Smoke-test one real stdio MCP server through `thndrs mcp test`,
      `thndrs mcp tools`, and `thndrs mcp call`, then record the exact config,
      command output shape, and any rough edges here.
- [x] Finish stdio-facing docs: config files, stdio setup, namespacing,
      diagnostics, security limits, and CLI commands.
- [x] Run and record the full validation set below after the docs and missing
      regression tests are complete.
- [x] Reassess this section; only start P7 after all stdio stability items are
      done.

Notes, 2026-07-05:

- Added `mcp::manager::tests::provider_catalog_includes_cached_mcp_stdio_tools`
  for provider catalog coverage.
- Added `mcp::adapter::tests::mismatched_protocol_version_is_a_diagnostic_not_failure`.
- Added `mcp::adapter::tests::startup_timeout_truncates_bounded_stderr_diagnostics`.
- Repeatable CLI output coverage remains in
  `tests::mcp_list_tools_and_call_use_fake_server`; output shape is:
  `mcp list` prints `<server>\t<enabled|disabled>\t<transport>`,
  `mcp tools <name>` prints `<namespaced-tool>\t<description>`, and
  `mcp call <server> <tool> --json <object>` prints tool output lines or
  `failed: <message>`.
- Real-server smoke tests passed with elevated permissions using temporary
  `.thndrs/mcp.toml` configs:

  ```toml
  [servers.memory]
  command = "npx"
  args = ["-y", "@modelcontextprotocol/server-memory"]
  timeout_secs = 60
  ```

  Commands and output shape:

  - `thndrs mcp test memory` printed
    `memory\tready\t9 tools`, plus diagnostics for negotiated protocol
    `2025-11-25` and server stderr.
  - `thndrs mcp tools memory` printed nine `mcp__memory__...` tools.
  - `thndrs mcp call memory read_graph --json '{}'` printed an empty graph:
    `{"entities":[],"relations":[]}` plus matching structured content.

  ```toml
  [servers.fs]
  command = "npx"
  args = ["-y", "@modelcontextprotocol/server-filesystem", "<temp-workspace>"]
  timeout_secs = 60
  ```

  Commands and output shape:

  - `thndrs mcp test fs` printed `fs\tready\t14 tools`, plus diagnostics for
    negotiated protocol `2025-11-25` and server stderr.
  - `thndrs mcp tools fs` printed fourteen `mcp__fs__...` tools.
  - `thndrs mcp call fs list_allowed_directories --json '{}'` printed the
    allowed temp workspace directories plus matching structured content.

## P7: HTTP Transport Follow-Up

- [x] Add HTTP transport only after P6.5 confirms stdio is stable.
- [x] Add `rmcp` Streamable HTTP client features.
- [x] Map URL and header config into the SDK HTTP transport.
- [x] Verify JSON POST and optional SSE behavior through SDK-backed fixtures.
- [x] Apply header redaction.
- [x] Apply response-size caps.
- [x] Add HTTP fixture tests.

Notes, 2026-07-05:

- Enabled `transport-streamable-http-client-reqwest`.
- `McpSdkClient::connect` now routes `stdio` and `streamable_http`.
- `streamable_http` maps `url` and validated `headers` into
  `StreamableHttpClientTransportConfig`.
- Added JSON and SSE local HTTP fixture tests:
  `streamable_http_client_initializes_lists_and_calls_json_fixture` and
  `streamable_http_client_accepts_sse_fixture_responses`.
- Header validation errors name the bad header but do not include header values.
- MCP call output caps are enforced by the shared manager sanitation path for
  stdio and HTTP transports.

## P8: Docs

- [x] Document MCP config files and examples.
- [x] Document that `thndrs` uses the official Rust MCP SDK for protocol and
      transport behavior while keeping local safety/audit policy.
- [x] Document stdio server setup.
- [x] Document tool namespacing.
- [x] Document failure diagnostics.
- [x] Document security limits and what MCP tools can access.
- [x] Update CLI reference with `mcp` commands.
- [x] Update tool docs with external tool behavior.

## Validation Commands

- [x] `cargo fmt` (passed 2026-07-05)
- [x] `cargo clippy --fix --allow-dirty --allow-staged` (passed 2026-07-05; required escalation because Cargo's lock listener could not bind inside the sandbox)
- [x] `cargo clippy` (passed 2026-07-05)
- [x] `cargo test mcp` (passed 2026-07-05)
- [x] `cargo test tools` (passed 2026-07-05)
- [x] `cargo test session` (passed 2026-07-05)
- [x] `cargo test cli` (passed 2026-07-05)
- [x] `cargo test` (passed 2026-07-05: 1158 passed, 0 failed, 4 ignored)
- [x] `pnpm --dir docs build` (passed 2026-07-05)
