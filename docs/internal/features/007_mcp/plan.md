# MCP Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-04

## Background

The old `thunderus` roadmap included a useful MCP plan: JSON-RPC clients,
stdio and HTTP transports, tool listing, tool calls, namespacing, config, and
debug commands.

That direction is worth keeping, but only after the built-in tool registry is
clean. MCP multiplies the number of available tools and failure modes, so adding
it before `006_tool_registry` would make the current central dispatch problem
worse.

## Problem

`thndrs` currently supports only built-in tools. Users cannot attach external
tool providers for project-specific APIs, browser automation, databases, or
internal systems.

The risk is overbuilding: MCP can easily become a plugin framework, permission
system, background process manager, and UI settings project all at once. This
feature keeps it to external tool discovery and invocation.

## Milestone Outcome

A user can configure MCP servers, list available MCP tools, and let the model
call namespaced MCP tools through the same registry/execution/session path as
built-in tools.

MCP failures are visible, bounded, and inspectable. Broken servers do not block
startup unless the user explicitly tests that server.

## Goals

1. Use the official Rust MCP SDK (`rmcp`) for protocol types, lifecycle, and
   transports unless a spike proves it cannot fit the synchronous `thndrs`
   runtime boundary.
2. Represent MCP tools as registry entries using the same execution result
   shape as built-in tools.
3. Namespace tool names as `mcp__{server}__{tool}`.
4. Add explicit user/project config files for MCP servers.
5. Add CLI/debug commands to list and test servers.
6. Record MCP calls in session JSONL.
7. Keep startup bounded and degrade clearly when servers fail.

## Transport Stages

### SDK Boundary

`modelcontextprotocol/rust-sdk` is the preferred implementation dependency.
Its `rmcp` crate is the official Rust SDK, exposes client support, includes
client-side stdio child-process transport, and also has Streamable HTTP client
features for the later HTTP stage.

Use the SDK for:

- MCP protocol models and serde behavior;
- initialize lifecycle and capability negotiation;
- `tools/list` and `tools/call` request/response plumbing;
- stdio child-process transport;
- Streamable HTTP transport when Stage 2 begins.

Keep these parts in `thndrs`:

- config file shape, merge rules, environment expansion, and redaction;
- server naming and `mcp__{server}__{tool}` namespacing;
- registry entries and provider tool catalog integration;
- tool approval policy, output caps, timeout policy, and session audit records;
- user-facing diagnostics and inspect/export formatting.

The first implementation task is a tiny spike that starts a fake stdio server,
initializes it with `rmcp`, lists tools, and calls one tool from a background
thread. If this requires turning the TUI or provider loop async, stop and
reconsider; MCP must stay behind the same thread/channel boundary used by other
long-running work.

### Stage 1: Stdio

Support through `rmcp`'s client-side child-process transport:

- spawn configured command;
- initialize handshake;
- `tools/list`;
- `tools/call`;
- shutdown on process exit;
- timeout and stderr capture.

### Stage 2: HTTP

Add HTTP transport only after stdio behavior is tested. Prefer `rmcp`'s
Streamable HTTP client transport instead of adding a parallel HTTP/JSON-RPC
implementation:

- JSON POST;
- SSE responses if required;
- header config with environment expansion;
- timeout and response-size caps.

## Configuration

Use separate MCP config files:

- user: `~/.thndrs/mcp.toml`
- project: `.thndrs/mcp.toml`

Project server definitions override user definitions by server name.

Example shape:

```toml
[servers.docs]
transport = "stdio"
command = "docs-mcp"
args = ["--workspace", "${THNDRS_WORKSPACE}"]
enabled = true
timeout_secs = 20
```

Environment expansion is allowed only for values. Unresolved variables skip the
server with a diagnostic.

Supported server config fields:

- `transport`: `"stdio"` or `"streamable_http"`; defaults to `"stdio"`.
- `command`: required for stdio servers.
- `args`: stdio argv entries after `command`.
- `env`: stdio child-process environment values.
- `url`: required for Streamable HTTP servers.
- `headers`: Streamable HTTP request headers.
- `enabled`: defaults to `true`.
- `timeout_secs`: defaults to `20` and must be greater than zero.

Server names must match `[A-Za-z0-9_-]+`. Registry tool names are assembled as
`mcp__{server}__{tool}`. The original server name and original MCP tool name are
kept in session metadata so exports remain inspectable even if provider-facing
names are normalized later.

The initial supported protocol version is `2025-06-18`. If a server negotiates
a different version, the adapter records the negotiated version in diagnostics
and session metadata; compatibility failures become ordinary server diagnostics
instead of startup panics.

Startup status is reported per server:

- `disabled`: configured but not eligible for discovery or calls.
- `skipped`: ignored before connection, such as unresolved environment values.
- `failed`: attempted but initialize/listing failed.
- `ready`: initialized and available for tool discovery.

## Public Commands

CLI:

- `thndrs mcp list`
- `thndrs mcp test <name>`
- `thndrs mcp tools <name>`
- `thndrs mcp call <server> <tool> --json <args>`

TUI:

- `mcp`: list configured servers and status.
- `mcp tools <name>`: list tools for one server.

## Decisions

- MCP implementation starts after `006_tool_registry` creates the shared tool
  execution and audit path.
- The official Rust SDK (`rmcp`) is the default implementation path for MCP
  protocol and transports. Local hand-written JSON-RPC types are added only for
  gaps that cannot be covered by the SDK without making the rest of `thndrs`
  more complex.
- MCP tools enter through the external-tool path instead of the built-in static
  registry. They are discovered at runtime, namespaced by server, and cannot
  rewrite built-in schemas, prompt identity, or local safety rules.
- Stdio is the first transport. Streamable HTTP is specified in the config
  schema now and implemented after stdio with the same lifecycle and audit
  rules.
- MCP tools use the same output caps, redaction, timeout, and session audit path
  as built-in tools.
- MCP configuration cannot rewrite built-in tool schemas, prompt identity, or
  harness safety rules.
- MCP stdio child processes are explicit configured server processes, not hidden
  background services.
- Tool approval uses the same user-visible execution policy as built-in tools;
  a separate MCP-specific permission system is not introduced.
- Marketplace, installer, and GUI surfaces are not part of the `thndrs` MCP
  path.

## Safety Rules

- MCP tool names are always namespaced.
- MCP arguments and outputs are capped.
- Server stderr is capped.
- Server startup and tool calls have timeouts.
- Environment variables in config are expanded explicitly and unresolved values
  skip the server.
- Secrets are never printed in diagnostics.
- Failed servers produce diagnostics but do not crash ordinary TUI startup.

## Dependencies

- `006_tool_registry` for registry-backed external tool entries.
- `003_configuration` for config-source diagnostics if shared helpers exist.
- `005_sessions` for inspect/export metadata.
- `rmcp` for MCP client protocol and transports. The stdio adapter should use
  `client` and `transport-child-process`; add
  `transport-streamable-http-client-reqwest` only in Stage 2.

## Verification

- SDK spike proving initialize, tools/list, and tools/call work against a fake
  stdio server from a background thread.
- Adapter tests that convert SDK initialize, tools/list, and tools/call results
  into `thndrs` diagnostics, registry definitions, and tool outputs.
- Stdio fake-server tests.
- Config merge and environment-expansion tests.
- Timeout and process-exit tests.
- Session tests for MCP calls.
- Prompt/tool catalog snapshot tests with namespaced tools.
