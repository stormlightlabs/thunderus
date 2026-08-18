---
title: "ACP Server"
---

ACP is a second frontend for the `thndrs` application. It has two roles:
`thndrs` can drive an external ACP agent from the TUI, or it can expose the
`thndrs` harness to an editor or IDE as an ACP agent server. Both roles use
newline-delimited JSON-RPC over stdio, but they own different parts of the
agent loop.

## Mental Model

```text
external ACP agent ── ACP stdio ──► core/acp runner ── AgentEvent ──► TUI
                                                                      │
                                                                      ▼
                                                               App + session

editor or IDE ─────── ACP stdio ──► server handlers ──► harness/provider/tools
                                                        │              │
                                                        └─ updates ◄───┘
                                                               │
                                                               ▼
                                                         ACP session log
```

In client mode, the external agent owns model requests and tool continuation.
`thndrs` owns the terminal UI, workspace checks for callbacks, permission
surfaces, cancellation, and local audit records.

In server mode, `thndrs` owns the provider-backed harness and tool continuation.
The ACP client owns the editor-facing transport, permission responses, and any
terminal process callback it provides. Ratatui is not involved in this mode.

## Responsibilities

- `core/acp` parses `acp:<name>` model ids, launches configured external agents,
  performs the ACP v1 handshake, maps session updates to application events,
  and implements client callbacks.
- `server` owns the ACP stdio transport, protocol request handlers, server-side
  session map, prompt conversion, event lowering, and client callbacks used by
  the server route.
- `cli/app` and `runtime` integrate client-mode events with the interactive
  state machine. The renderer displays them through the normal transcript and
  live surfaces.
- `core/harness`, `core/agent`, `core/providers`, and `core/mcp` provide the
  provider-backed run, tool, MCP, and cancellation behavior used by the server
  route and by the rest of the application.
- `core/session` writes normalized transcript, tool, side-effect, permission,
  and ACP identity records when session persistence is enabled.
- `core/acp/registry` reads ACP Registry metadata and writes reviewed config
  blocks. It does not execute package managers or launch an agent during
  discovery or install.

## Shared Runtime

The two ACP roles share application policy rather than sharing one agent loop.
Both use the same workspace path rules, timeout-oriented diagnostics, bounded
and redacted tool output, cooperative cancellation, MCP configuration model,
and session record types.

The client route enters through `runtime::spawn_agent`. It passes the active
prompt and effective MCP configuration to `core/acp::runner`, then receives
normalized `AgentEvent` values through the same runtime channel used by a
built-in run. It does not build the built-in provider prompt or execute the
external agent's model requests itself.

The server route enters through `server::handlers::prompt`. It builds an
`AgentRunConfig`, attaches the session's stdio MCP servers, lowers the ACP
prompt to provider-neutral messages, and starts a shared `HarnessTurn`. The
current server prompt path uses the ACP content blocks and a minimal prompt
bundle; it does not run the TUI's context-ledger or skill-selection path.

## Transport and Session Handling

### External-agent client

A configured `[acp_agents.<name>]` entry supplies a command, argv values,
non-secret environment values, enabled state, and a per-request timeout. The
runner launches the command as a local stdio child with `AcpAgent::from_args`.
Agent stderr becomes diagnostic status; stdout carries ACP JSON-RPC.
Only ACP protocol version 1 is accepted.

The runner initializes with filesystem read/write and terminal client
capabilities. If the agent advertises authentication methods, the runner uses
its first method. It then creates a new external ACP session for the selected
workspace and sends the prompt. Admin commands such as logout and external
session list/load/resume/close open their own short-lived ACP connection and
require the advertised capability.

The runner offers effective MCP configuration in `session/new`. Enabled stdio
servers are representable directly. Streamable HTTP servers are passed only
when the external agent advertises HTTP MCP support; disabled or unsupported
servers produce diagnostics instead of being started by `thndrs`.

### ACP agent server

`server::run_stdio` builds the line transport. stdout is reserved for JSON-RPC
responses and notifications; diagnostics belong on stderr. The server stores
opaque ACP ids separately from local session ids. A new session validates and
canonicalizes the absolute `cwd` supplied by the client, creates an
`acp-session-00000001`-style id, and may attach a local append-only JSONL
writer.

The server supports session creation, listing, loading, resuming, closing,
deleting, and per-session model and reasoning options. `session/list` combines
persisted JSONL files with in-memory sessions and filters by cwd. `session/load`
replays normalized records as `session/update` notifications; `session/resume`
attaches the writer without replaying history. `session/delete` removes
in-memory state but does not remove a persisted JSONL file and therefore fails
when such a file exists.

A session admits one prompt turn at a time. `session/cancel` marks the session,
cancels its active token, and lets the prompt handler return a cancelled stop
reason. A prompt can include text, images, resource links, and embedded text or
blob resources. Audio blocks and other unsupported blocks return a protocol
error.

## Request and Event Flow

### Client mode

1. The interactive runtime selects `acp:<name>`, finds the workspace root, and
   starts a `RunHandle` with the active prompt and effective MCP configuration.
2. The runner initializes ACP v1, authenticates when the agent advertises an
   auth method, creates an external session, and sends a text prompt.
3. `session/update` notifications become `AgentEvent` values. Assistant and
   thought chunks update the transcript; tool updates update the normal tool
   lifecycle; usage, plans, and status updates become application status or
   accounting events.
4. An external agent's permission request opens the TUI permission surface.
   The selected option or cancellation is sent back to the agent and recorded
   as redacted ACP permission metadata.
5. `fs/read_text_file` and `fs/write_text_file` resolve paths under the
   workspace root. Reads require bounded UTF-8 files; writes create missing
   parent directories and produce the normal file-write audit.
6. Terminal callbacks run child processes in the workspace or a contained
   subdirectory. Output is capped and redacted, process events become tool
   records, and the registry kills unreleased processes when the run ends.
7. Local cancellation or a timeout sends `session/cancel` when a session exists.
   The runner maps the final stop reason to `Finished`, `Cancelled`, or
   `Failed`, and the interactive lifecycle settles the worker.

### Server mode

1. `initialize` negotiates ACP v1, stores client information and capabilities,
   and advertises image and embedded-context prompts, MCP, and session
   management support.
2. `session/new` validates the cwd, converts only stdio MCP server definitions
   into a session-local `McpConfig`, creates local session metadata, and returns
   model and reasoning config options.
3. `session/prompt` assembles the supported ACP content blocks, records the user
   turn, and starts a provider-backed harness turn with the session's model,
   reasoning controls, authority, reduction policy, and MCP manager.
4. The harness emits normalized `AgentEvent` values. The server persists tool
   and side-effect records, maps text, thought, usage, tool, failure, and
   cancellation events to `session/update`, and returns the final stop reason.
5. Workspace writes, shell commands, and MCP calls use the server permission
   hook. It asks the ACP client for allow-once or reject-once and does not
   persist blanket approvals. If the client advertises terminal support,
   `run_shell` uses the client's terminal callbacks; otherwise the server's
   normal tool path executes it locally.

## Boundaries

- `core/acp` owns the external-agent client adapter. It does not own TUI state
  transitions, provider wire conversion, or Ratatui rendering.
- `server` owns ACP JSON-RPC transport and server-session policy. It does not
  poll terminal input or render the terminal UI.
- `cli/app`, `runtime`, and `cli/renderer` own the interactive frontend. The
  server frontend must not call into TUI presentation code.
- `core/harness` and `core/agent` own provider-backed continuation for the
  server route. An external ACP agent owns continuation for the client route.
- `core/mcp` owns MCP config loading, connection startup, namespaced tool
  routing, resource limits, and call output reduction. ACP only converts the
  transport's server declarations at its boundary.
- `core/session` owns append-only persistence. ACP ids, client metadata, and
  normalized audits may be recorded; raw JSON-RPC lines, credentials, and
  provider-private payloads are not session contracts.
- Filesystem and terminal callbacks enforce workspace containment at the
  callback boundary. An ACP permission response does not create an OS sandbox.

## Key Types

- `AcpAgentConfig` — one configured external ACP command and timeout policy.
- `RunHandle` — client-mode workspace, prompt, MCP config, cancellation, and
  external-agent event stream.
- `AgentEvent` — provider-neutral application event vocabulary used by the TUI
  and ACP adapters.
- `ListedSession` and `AcpSessionMetadata` — external session projections and
  recorded ACP identity metadata.
- `PendingPermission` — one client-mode permission request owned by the TUI.
- `TerminalRegistry` — client-mode ACP terminal process registry and audit path.
- `ServerConfig` and `ServerState` — ACP server process configuration and shared
  session/request state.
- `AcpSessionStore` and `AcpServerSession` — opaque ACP ids, local metadata,
  cwd, turn guard, writer, and session MCP configuration.
- `SessionUpdateIntent` — protocol-independent server event projection before
  lowering to ACP `SessionUpdate` values.

## Invariants

- ACP transport payloads stay inside `core/acp` or `server`; public library
  contracts expose normalized events and application metadata instead.
- Client-mode ACP lifecycle requests accept ACP v1 only and are subject to the
  configured timeout or cooperative cancellation token. Server-side prompt
  work uses the shared provider, tool, and cancellation paths.
- One ACP server session has at most one active prompt turn. External ACP
  session ids and local JSONL session ids remain distinct.
- Client filesystem and terminal callbacks stay inside the selected workspace;
  their outputs and recorded arguments are bounded and redacted.
- Server-side permission requests are allow-once or reject-once. A client
  disconnect or cancelled turn rejects the pending operation.
- MCP configuration is passed or converted at session creation; ACP does not
  install packages or grant project trust. Server-mode MCP calls use server
  tool authority; an external client-mode agent owns processes it starts itself.
- Server stdout contains protocol lines only. Human diagnostics and tracing use
  stderr.

## Source Map

| Responsibility                              | Primary source                                                    |
| ------------------------------------------- | ----------------------------------------------------------------- |
| ACP model routing and config helpers        | `crates/thndrs/src/core/acp/config.rs`                            |
| External-agent lifecycle and MCP projection | `crates/thndrs/src/core/acp/runner.rs`                            |
| Client update normalization                 | `crates/thndrs/src/core/acp/events.rs`                            |
| Client filesystem callbacks                 | `crates/thndrs/src/core/acp/fs.rs`                                |
| Client permission state                     | `crates/thndrs/src/core/acp/permissions.rs`                       |
| Client terminal processes                   | `crates/thndrs/src/core/acp/terminal.rs`                          |
| Registry discovery and managed config       | `crates/thndrs/src/core/acp/registry.rs`                          |
| TUI ACP route                               | `crates/thndrs/src/runtime/interactive.rs:spawn_agent`            |
| ACP server transport                        | `crates/thndrs/src/server/mod.rs` and `handlers.rs:run_stdio`     |
| Server request and session handlers         | `crates/thndrs/src/server/handlers.rs`                            |
| Server session identity and turn guard      | `crates/thndrs/src/server/session.rs`                             |
| Server event projection                     | `crates/thndrs/src/server/events.rs`                              |
| Server config options                       | `crates/thndrs/src/server/config_options.rs`                      |
| ACP and normalized session records          | `crates/thndrs/src/core/session/`                                 |
| CLI ACP commands                            | `crates/thndrs/src/cli/commands/acp.rs` and `runtime/commands.rs` |

## Related

- [ACP usage](/docs/usage/acp/)
- [Runtime and state](/docs/internals/runtime/)
- [Request lifecycle](/docs/internals/lifecycle/)
- [Context assembly](/docs/internals/context/)
- [Tools](/docs/internals/tools/)
- [Sessions](/docs/internals/sessions/)
- [Codebase tour](/docs/internals/codebase/)
