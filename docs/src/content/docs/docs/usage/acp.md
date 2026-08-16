---
title: "ACP"
---

`thndrs` can act as an Agent Client Protocol client for an external ACP agent.
The external agent owns its model loop, while `thndrs` owns the terminal UI,
workspace boundary, permission prompts, cancellation, and session records.

ACP agents are configured in normal `thndrs` TOML config and selected with the
model id form `acp:<name>`.

`thndrs` can also act as an ACP agent server through `thndrs acp serve`. In
that mode an editor or IDE launches `thndrs`, sends ACP requests over stdio,
and receives streamed session updates while the normal `thndrs` harness,
providers, tools, project context, and session records do the work.

## Configuration

Add one table per agent under `[acp_agents.<name>]` in `~/.thndrs/config.toml`
or `.thndrs/config.toml`:

```toml
[acp_agents.codex]
command = "npx"
args = ["-y", "@zed-industries/codex-acp@latest"]
env = {}
enabled = true
timeout_secs = 60
```

Agent names may contain ASCII letters, digits, `_`, and `-`. The same name is
used after `acp:` when selecting the model.

Supported keys:

- `command`: executable to launch. Required.
- `args`: argv entries passed after `command`; defaults to `[]`.
- `env`: non-secret environment variables passed to the child process; defaults
  to `{}`.
- `enabled`: whether the agent can be selected; defaults to `true`.
- `timeout_secs`: initialize, authentication, session, prompt, and admin
  command timeout; defaults to `60`.

Project config overrides global config by agent name. Unknown keys are errors.
Use `env` for non-secret child process settings only. Values in `env` are
redacted in diagnostics and session metadata, but secret-shaped keys are still
rejected anywhere in TOML, including under `acp_agents.<name>.env`. Put agent
credentials in the parent process environment, the agent's own login/auth flow,
or an explicit wrapper command outside `thndrs` TOML.

ACP currently supports stdio agents only. The configured command is launched as
a local child process and must speak ACP JSON-RPC over stdin/stdout. Use stderr
for agent logs.

## Selecting An Agent

Start the TUI with an ACP model id:

```sh
thndrs --model acp:codex
```

When you submit a prompt, `thndrs` launches the configured agent, initializes
ACP v1, authenticates through agent-owned auth methods if advertised, creates
an external ACP session for the selected workspace, sends the prompt, streams
updates into the normal transcript, and records ACP metadata in the local
session JSONL.

Unknown, disabled, malformed, or failing agents are reported as normal startup
or run failures.

## Running The Agent Server

Use `thndrs acp serve` when another ACP client, usually an editor, should drive
the `thndrs` harness:

```sh
thndrs --cwd /path/to/project acp serve
```

The server speaks newline-delimited ACP JSON-RPC over stdio. stdout is reserved
for protocol messages only; diagnostics and tracing go to stderr. The process
exits when stdin closes or the ACP connection shuts down.

Supported flags:

- `--cwd <path>`: default workspace when the client does not provide a more
  specific session cwd.
- `--model <model>`: provider model for new ACP sessions.
- `--websearch <duckduckgo|searxng|none>`: application-owned web-search backend.
- `--websearch-url <url>`: SearXNG HTTP(S) base URL.
- `--session-dir <path>`: directory for append-only local session JSONL files.

The binary also reads the normal `thndrs` config layers. CLI flags override
environment and TOML config.

### Editor Setup

Configure the editor's ACP agent command to launch the binary with stdio. The
exact key names vary by client, but the shape should be:

```json
{ "agents": { "thndrs": { "command": "thndrs", "args": ["--cwd", "/path/to/project", "acp", "serve"] } } }
```

For per-project use, prefer an absolute `--cwd` and keep provider credentials in
the parent process environment or the workspace `.env` file. Do not wrap the
server in commands that print banners or progress text to stdout.

## Server Capabilities

`thndrs acp serve` supports these ACP v1 agent behaviors:

- `initialize`, `session/new`, `session/prompt`, `session/cancel`, and
  streamed `session/update`.
- Agent-owned session lifecycle: `session/list`, `session/load`,
  `session/resume`, `session/close`, and non-destructive `session/delete`.
- Text prompts, image prompt blocks, resource links, and embedded text/blob
  resources. Audio prompt blocks are rejected.
- Session config options for model, web-search backend, and SearXNG base URL.
- Tool-call updates for read, search, edit, fetch, shell, thinking, and other
  tools.
- Permission requests before local file writes and shell commands.
- Client filesystem callbacks when the client advertises `fs/read_text_file` or
  `fs/write_text_file`.
- Client terminal callbacks when the client advertises `terminal`.
- Stdio MCP server config passed through `session/new`; HTTP and SSE MCP
  transports are rejected.

Unsupported content or malformed requests return protocol errors. If provider
credentials or config are missing, the prompt fails as a normal agent failure
and the error is reported through ACP updates and the final prompt response.

## Permission Prompts

An ACP agent can ask the client to choose from agent-provided permission
options. `thndrs` shows one focused permission prompt at a time.

Use Up/Down to move between options and Enter to select the highlighted option.
Escape cancels the permission request. While a permission prompt is open, normal
prompt submission is blocked until the request is answered or cancelled.

Permission request and outcome metadata are written to the local session record.
Credentials and raw protocol stdio lines are not stored.

When `thndrs acp serve` is the agent, the direction is reversed:
`thndrs acp serve` asks the editor client for permission before file writes and
shell commands. The initial options are allow once and reject once. Blanket
approvals are not persisted. If the client rejects the request, cancels it, or
disconnects before answering, the tool operation is rejected or the active prompt
is cancelled.

## Server Session Mapping

ACP session ids are opaque client-facing ids such as
`acp-session-00000001`. Local `thndrs` session ids remain separate and are used
for JSONL files, inspect/export commands, and local audit records.

When `--session-dir` is configured, each ACP-created session writes an
`acp_session` metadata record containing the ACP session id, local session id,
agent/server name, protocol version, and client info from `initialize`.
Permission request and outcome records are written to the same local session
log.

## Supported Capabilities

`thndrs` supports these ACP v1 client behaviors:

- Stdio ACP agents launched from configured `command` and `args`.
- Agent-owned authentication through advertised ACP auth methods.
- Assistant, reasoning, status, usage, and tool-call session updates mapped into
  the normal transcript.
- `read_text_file` and `write_text_file` callbacks that reject paths outside
  the workspace.
- User permission prompts with selected or cancelled outcomes.
- Local cancellation through `session/cancel`.
- Terminal callbacks that reject requested working directories outside the
  workspace, cap and redact output, show visible tool rows, clean up, and write
  session audit records.
- Effective user-plus-project MCP server config passed through `session/new`
  when the agent advertises MCP support and the server fits ACP's stable
  `mcpServers` shape.
- Agent-owned session commands when the agent advertises support:
  `session/list`, `session/load`, `session/resume`, and `session/close`.
- Agent-owned logout when the agent advertises support.

Unsupported ACP behavior fails closed with a status update, protocol error, or
command failure:

- Remote, Streamable HTTP, WebSocket, or custom ACP transports.
- A `thndrs`-provided MCP self-proxy.
- ACP agents choosing arbitrary MCP servers outside the effective `thndrs` MCP
  config.
- ACP registry install or update.
- Client-owned ACP credential storage.
- Unsaved editor buffer access.

## Commands

List configured agents:

```sh
$ thndrs acp list

codex   enabled    npx -y @zed-industries/codex-acp@latest
```

Inspect one agent:

```sh
thndrs acp inspect codex
```

This prints enabled status, the redacted command display, argv entries,
environment variable names without values, timeout, and config source.

Smoke test one agent without opening the TUI:

```sh
thndrs acp smoke codex --prompt "Say hello"
```

The smoke command initializes the agent, creates a temporary external ACP
session for the selected workspace, sends one prompt, prints streamed events,
and exits. If an agent requests permission during a smoke run, the request is
printed and cancelled.

Typical smoke output includes startup status rows, any stderr diagnostics,
agent info, the external ACP session id, streamed assistant text, and
`finished` on success.

Agent-owned auth and sessions are available through:

```sh
thndrs acp logout codex
thndrs acp list-sessions codex
thndrs acp load-session codex <session-id>
thndrs acp resume-session codex <session-id>
thndrs acp close-session codex <session-id>
```

These commands fail when the agent does not advertise the corresponding ACP
capability.

`list-sessions` prints tab-separated `session-id`, `cwd`, `title`, and
`updated-at` fields. `load-session` prints replayed updates, then
`loaded: <agent> <session-id>` and the `acp_session` metadata line.

List available agents from the ACP Registry:

```sh
thndrs acp registry
```

This fetches the official registry metadata from
`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`, prints
available agents, and does not install or update anything. For offline
inspection or tests, pass a local registry JSON file:

```sh
thndrs acp registry --file registry.json
```

The registry command treats registry entries as discovery metadata only.
Package names, archive URLs, args, and env keys are not executed by
`acp registry`. Env values from registry metadata are ignored and never printed.

Install a supported `npx` or `uvx` registry agent into the current workspace:

```sh
thndrs acp install codex-acp --name codex --yes
```

This writes a managed `[acp_agents.<name>]` block to `.thndrs/config.toml` and
records source/version metadata in `.thndrs/acp-installed.toml`. Install does
not run the package manager, launch the agent, log in, smoke test, or install
binary archives. Binary-only registry entries fail closed until archive
verification is supported.

Update a registry-managed agent from the latest registry metadata:

```sh
thndrs acp update codex --yes
```

Use `--file registry.json` with either command to install or update from a
local registry file. Existing manually configured ACP agents are not overwritten;
only blocks previously created by `thndrs acp install` are updated.
