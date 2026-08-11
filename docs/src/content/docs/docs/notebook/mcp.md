---
title: "MCP Lifecycle"
sources:
  - https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle
  - https://modelcontextprotocol.io/specification/2025-06-18/basic/transports
  - https://modelcontextprotocol.io/specification/2025-06-18/server/tools
Author: Model Context Protocol maintainers
Date: 2025-06-18
Captured: 2026-07-04
Tags: [mcp, json-rpc, tools, stdio, streamable-http]
---

## Summary

MCP defines a JSON-RPC lifecycle for connecting clients to external tool and
context servers, with explicit initialization, capability negotiation,
transport rules, tool discovery, tool calls, and shutdown behavior.

## Key Ideas

- **Lifecycle first:** Clients initialize before normal operation, negotiate a
  protocol version and capabilities, then send an initialized notification.
- **Stdio is process-based:** The client launches a server subprocess and
  exchanges newline-delimited JSON-RPC over stdin/stdout; stderr is for logs.
- **Streamable HTTP supersedes old HTTP+SSE:** The current HTTP transport uses
  POST for client messages and optional SSE for streamed server messages.
- **Tools are discoverable:** Servers expose callable tools through
  `tools/list`, then clients invoke them with `tools/call`.
- **Timeouts are part of correctness:** Implementations should use request
  timeouts and cancellation to avoid resource exhaustion.

## Claims & Evidence

### MCP initialization is mandatory before ordinary requests.

The lifecycle spec says initialization is the first interaction, carrying the
client protocol version, capabilities, and client information. The server
responds with its supported protocol version, capabilities, server information,
and optional instructions.

Confidence: high.

### Stdio requires protocol-clean stdout.

The transport spec requires stdout to contain only valid MCP messages, while
stderr may carry UTF-8 log strings. This means clients need strict stdout
parsing and bounded stderr capture.

Confidence: high.

### MCP tools are model-facing but require host trust controls.

The tools spec says tools are designed for model-controlled invocation, while
applications should make exposed tools clear and keep a human in the loop for
sensitive operations.

Confidence: high.

## Important Terms

| Term            | Meaning                                                    |
| --------------- | ---------------------------------------------------------- |
| JSON-RPC        | Request/response/notification protocol used by MCP.        |
| `initialize`    | First request used for version and capability negotiation. |
| `tools/list`    | Request for discovering server tools and their schemas.    |
| `tools/call`    | Request for invoking a named server tool with arguments.   |
| Stdio transport | Local subprocess transport using stdin/stdout.             |
| Streamable HTTP | HTTP transport using POST and optional SSE.                |

## Questions for Review

- What timeout defaults should apply to initialize, tools/list, and tools/call?
- How should `notifications/tools/list_changed` invalidate cached schemas?
- How should stderr diagnostics appear in logs versus transcript rows?
- How should MCP tool calls be represented in session export records?

## Connections

- Related ideas: tool registry, external tools, session audit, command
  diagnostics.
- Related sources: OpenCode MCP docs, Claude Code MCP docs, MCP spec.

## Open Questions

- Should stdio servers start eagerly during app startup or lazily on first use?
- How should Streamable HTTP session ids be stored and redacted?
- How much server-provided tool annotation metadata should be shown to users?

## Takeaways

- MCP clients need explicit initialization, capability negotiation, timeouts,
  cancellation, and process cleanup.
- Stdio clients must keep protocol traffic on stdout and treat stderr as
  diagnostic output.
- Hosts should show users which tools a server exposes and when those tools are
  invoked.
