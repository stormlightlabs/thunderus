---
title: "Agent Client Protocol"
Notes: Agent Client Protocol
Author: Agent Client Protocol maintainers
Date: captured 2026-07-04
Captured: 2026-07-04
Tags: [acp, json-rpc, coding-agent, client-protocol, mcp, tools]
Sources:
  - https://agentclientprotocol.com/get-started/
---

## Summary

ACP standardizes the boundary between coding-agent clients and coding-agent
implementations: clients own the user interface and environment, agents own the
model loop, and both sides communicate with JSON-RPC methods and notifications.

## Key Ideas

- **Editor-agent decoupling:** ACP gives editors, IDEs, TUIs, and other clients
  one integration shape for many coding agents instead of per-agent adapters.
- **Bidirectional JSON-RPC:** Both sides expose callable methods. Clients call
  agent methods such as `initialize`, `session/new`, and `session/prompt`;
  agents call client methods such as `session/request_permission`,
  `fs/read_text_file`, `fs/write_text_file`, and `terminal/create`.
- **Stdio is the baseline transport:** The common local setup is a client that
  launches an agent subprocess and exchanges newline-delimited JSON-RPC over
  stdin/stdout. stdout must contain only ACP messages; stderr is for logs.
- **Sessions are the unit of work:** A connection can host multiple sessions.
  Each session has its own id, working directory, MCP server config, history,
  configuration state, and prompt turns.
- **Prompt turns stream state, not just text:** A `session/prompt` request stays
  open while the agent emits `session/update` notifications for plans, message
  chunks, tool calls, usage, commands, config changes, and mode changes.
- **The client remains the trust boundary:** The agent may execute tools, but
  ACP makes permission requests, filesystem access, terminal execution, diffs,
  and live output visible through client-owned surfaces.
- **MCP is complementary:** ACP reuses MCP content concepts and lets the client
  pass MCP server configs to the agent. ACP is the editor-agent protocol; MCP is
  still the tool/data-server protocol.

## Implementation Shape

1. Start an ACP connection, usually by launching the agent subprocess.
2. Send `initialize` with the latest supported protocol version,
   `clientCapabilities`, and optional `clientInfo`.
3. Read the agent's selected protocol version, `agentCapabilities`, optional
   `agentInfo`, and `authMethods`. Treat omitted capabilities as unsupported.
4. If authentication is required, call `authenticate` with one advertised method
   id before creating protected sessions.
5. Create or attach to a session:
   - `session/new` creates a fresh conversation.
   - `session/load` resumes with history replayed through `session/update`.
   - `session/resume` reconnects without replay when the capability exists.
   - `session/close` cancels active work and frees agent-side resources when
     the capability exists.
6. Send `session/prompt` with a `ContentBlock[]` user message.
7. Render streamed `session/update` notifications while the prompt is active.
8. Answer agent-to-client requests, especially permission, filesystem, and
   terminal requests, according to user settings and client policy.
9. End the turn when `session/prompt` returns a `stopReason`.

## Claims & Evidence

### ACP's core value is interoperability between clients and agents.

The introduction frames ACP as doing for coding agents what LSP did for language
servers: agents that implement the protocol can work with compatible clients,
and clients can support many agents through one standard interface.

Caveat/confidence: High. The public agents, clients, and registry pages show a
large and growing ecosystem, but implementation quality will still vary by
agent and client.

### Initialization is the compatibility gate.

The initialization docs make version negotiation and capability exchange the
first required step before sessions. The protocol version is a major-version
integer, while non-breaking feature support is represented through capability
objects.

Caveat/confidence: High.

### Capabilities must be treated strictly.

Client filesystem and terminal methods are only available when the client
advertises those capabilities. Agent features such as session load, resume,
close, config options, MCP HTTP/SSE support, and logout are likewise gated by
agent capabilities.

Caveat/confidence: High.

### The protocol is designed around live UI updates.

`session/update` carries agent messages, user replay chunks, plans, tool calls,
usage updates, config changes, mode updates, and slash-command availability.
This makes ACP closer to an interactive agent UI protocol than a plain chat
completion protocol.

Caveat/confidence: High.

### Tool calls are reported by the agent even when execution details vary.

ACP models tool calls with ids, human-readable titles, kinds, statuses,
content, locations, raw input, and raw output. Tool output can include ordinary
content blocks, diffs, or embedded terminal output.

Caveat/confidence: High.

### Filesystem and terminal methods are client-environment affordances.

The filesystem methods let agents read and write text files through the client,
including unsaved editor state. Terminal methods let agents create, observe,
wait for, kill, and release commands while respecting output byte limits.

Caveat/confidence: High.

### Session configuration options supersede the older modes API.

Session config options expose selectable settings such as mode, model, model
config, and reasoning level. The docs say clients should prefer config options
when present and fall back to the older `modes` field during the transition.

Caveat/confidence: High.

### Remote transport support is not the stable center of gravity yet.

The protocol docs define stdio and mention Streamable HTTP as draft/in-progress.
Custom transports are allowed if they preserve JSON-RPC message format and ACP
lifecycle requirements.

Caveat/confidence: High for current docs; this area is actively evolving.

## Important Terms

| Term              | Meaning                                                                                             |
| ----------------- | --------------------------------------------------------------------------------------------------- |
| ACP               | Agent Client Protocol, the protocol between coding-agent clients and agents.                        |
| Client            | The user-facing app, usually an editor, IDE, TUI, desktop app, web UI, or bridge.                   |
| Agent             | The coding-agent implementation that runs the model loop and reports progress.                      |
| JSON-RPC          | Request/response/notification envelope used for ACP messages.                                       |
| `initialize`      | First compatibility handshake for version, capabilities, implementation info, and auth methods.     |
| Capability        | A feature advertisement; omitted capabilities must be treated as unsupported.                       |
| Session           | A conversation/work context with an id, cwd, MCP config, and independent state.                     |
| Prompt turn       | One `session/prompt` request plus streamed updates until a stop reason is returned.                 |
| `session/update`  | Agent-to-client notification for streamed UI state.                                                 |
| Content block     | MCP-compatible user-facing content such as text, image, audio, embedded resource, or resource link. |
| Tool call         | Agent-reported action requested by the model, with status, content, and optional permission flow.   |
| MCP server config | Tool/data-server connection details passed by the client for the agent to use.                      |
| Config option     | Session-level selector exposed by the agent, such as model, mode, or reasoning level.               |
| ACP Registry      | Curated metadata registry for distributing ACP-compatible agents.                                   |

## Implementation Notes

- Keep transport parsing strict: one JSON-RPC message per newline on stdio, no
  embedded newlines, no non-protocol stdout.
- Model the connection as bidirectional. A prompt request may be pending while
  the agent calls back into the client for permissions, files, or terminals.
- Store session ids as opaque values. Do not infer semantics from their shape.
- Require absolute paths for protocol paths. `cwd` remains the primary base for
  relative resolution, and additional workspace roots are explicit.
- Handle cancellation as a normal outcome. `session/cancel` is a notification,
  pending permission requests should resolve as cancelled, and the prompt should
  finish with the `cancelled` stop reason.
- Render tool calls incrementally. A tool may start as `pending`, become
  `in_progress`, then finish as `completed` or `failed`; updates only include
  changed fields.
- Treat terminal resources as lifecycle-managed handles. Created terminals
  must be released, and released terminal ids are invalid for future methods.
- Preserve complete config state updates. When a config option changes, the
  response or notification carries the complete current list, allowing dependent
  option changes.
- Use `_meta` and underscore-prefixed methods for extensions. Never add custom
  root fields to standard ACP objects.
- Prefer an SDK where available. Official docs point to Rust, TypeScript, and
  Python libraries that implement both agent and client sides.

## Questions For Review

- What is the minimum ACP subset needed for a useful client: initialize,
  session/new, session/prompt, session/update, permissions, and cancellation?
- How should a client represent concurrent sessions that share one subprocess
  connection?
- Which session updates should be transcript records, and which should be
  ephemeral UI state?
- How should permission decisions be persisted across sessions without hiding
  sensitive actions from users?
- How should filesystem methods reconcile unsaved editor buffers, on-disk
  files, generated diffs, and external file changes?
- Where should terminal output truncation, release, and final display state be
  recorded?
- Should ACP integration expose MCP servers by passing user-configured servers
  directly, by proxying through the client, or both?
- What parts of config options map to local UI controls, keybindings, and
  session metadata?

## Connections

- Related ideas: LSP-style decoupling, MCP tool servers, tool-call audit trails,
  session replay, permission UX, terminal lifecycle management.
- Related sources: [`mcp`](/docs/notebook/mcp), [`sessions`](/docs/notebook/sessions),
  [`harness-engineering`](/docs/notebook/harness-engineering), [`context-control`](/docs/notebook/context-control),
  [`notifications`](/docs/notebook/notifications).
- Useful applications: agent interoperability, adapter design, client-side
  transcript model, external agent subprocess management, tool-call rendering.

## Open Questions

- How stable will Streamable HTTP and WebSocket transport semantics become, and
  what client abstractions will survive that transition?
- How should clients version or migrate stored ACP session transcripts as the
  protocol evolves?
- What should happen when an agent supports both client filesystem methods and
  direct MCP filesystem servers?
- How much raw tool input/output should a client persist for auditability
  versus redaction and storage cost?
- What error taxonomy beyond JSON-RPC errors is needed for user-facing
  recovery, especially auth, cancellation, process crashes, and unsupported
  capabilities?

## Takeaways

- ACP is a client-agent UI protocol, not just a transport for prompts.
- The essential implementation problem is bidirectional session orchestration
  with strict capability checks and visible tool state.
- Stdio support is the practical first target; remote transports and custom
  extensions should sit behind the same JSON-RPC lifecycle.
