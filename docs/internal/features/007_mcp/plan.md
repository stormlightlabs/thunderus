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
it before `007_tool_registry` would make the current central dispatch problem
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

1. Implement a minimal MCP client for stdio servers first.
2. Represent MCP tools as registry entries using the same execution result
   shape as built-in tools.
3. Namespace tool names as `mcp__{server}__{tool}`.
4. Add explicit user/project config files for MCP servers.
5. Add CLI/debug commands to list and test servers.
6. Record MCP calls in session JSONL.
7. Keep startup bounded and degrade clearly when servers fail.

## Transport Stages

### Stage 1: Stdio

Support:

- spawn configured command;
- initialize handshake;
- `tools/list`;
- `tools/call`;
- shutdown on process exit;
- timeout and stderr capture.

### Stage 2: HTTP

Add HTTP transport only after stdio behavior is tested:

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

- MCP implementation starts after `007_tool_registry` creates the shared tool
  execution and audit path.
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

- `007_tool_registry` for registry-backed external tool entries.
- `003_configuration` for config-source diagnostics if shared helpers exist.
- `005_sessions` for inspect/export metadata.

## Verification

- Protocol fixture tests for initialize, tools/list, and tools/call.
- Stdio fake-server tests.
- Config merge and environment-expansion tests.
- Timeout and process-exit tests.
- Session tests for MCP calls.
- Prompt/tool catalog snapshot tests with namespaced tools.
