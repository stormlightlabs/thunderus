---
title: "ACP"
---

`thndrs` can act as an Agent Client Protocol client for an external ACP agent.
The external agent owns its model loop, while `thndrs` owns the terminal UI,
workspace boundary, permission prompts, cancellation, and session records.

ACP agents are configured in normal `thndrs` TOML config and selected with the
model id form `acp:<name>`.

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
- `env`: environment variables passed to the child process; defaults to `{}`.
- `enabled`: whether the agent can be selected; defaults to `true`.
- `timeout_secs`: initialize, authentication, session, prompt, and admin
  command timeout; defaults to `60`.

Project config overrides global config by agent name. Unknown keys are errors.
Secret-shaped TOML keys are rejected. Values in `env` are allowed but are
redacted in diagnostics and session metadata.

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

## Permission Prompts

An ACP agent can ask the client to choose from agent-provided permission
options. `thndrs` shows one focused permission prompt at a time.

Use Up/Down to move between options and Enter to select the highlighted option.
Escape cancels the permission request. While a permission prompt is open, normal
prompt submission is blocked until the request is answered or cancelled.

Permission request and outcome metadata are written to the local session record.
Credentials and raw protocol stdio lines are not stored.

## Supported Capabilities

`thndrs` supports these ACP v1 client behaviors:

- Stdio ACP agents launched from configured `command` and `args`.
- Agent-owned authentication through advertised ACP auth methods.
- Assistant, reasoning, status, usage, and tool-call session updates mapped into
  the normal transcript.
- Workspace-contained `read_text_file` and `write_text_file` callbacks.
- User permission prompts with selected or cancelled outcomes.
- Local cancellation through `session/cancel`.
- Terminal callbacks with workspace-contained cwd handling, output caps,
  redaction, visible tool rows, cleanup, and session audit records.
- Agent-owned session commands when the agent advertises support:
  `session/list`, `session/load`, `session/resume`, and `session/close`.
- Agent-owned logout when the agent advertises support.

Unsupported ACP behavior fails closed with a status update, protocol error, or
command failure:

- Remote, Streamable HTTP, WebSocket, or custom ACP transports.
- MCP-over-ACP server injection.
- ACP registry install or update.
- `thndrs` acting as an ACP agent server.
- Client-owned ACP credential storage.
- Unsaved editor buffer access.

## Commands

List configured agents:

```sh
thndrs acp list
```

Output is tab-separated:

```text
codex    enabled    npx -y @zed-industries/codex-acp@latest
```

Inspect one agent:

```sh
thndrs acp inspect codex
```

This prints enabled status, redacted command details, environment variable
names, timeout, and config source.

Smoke test one agent without opening the TUI:

```sh
thndrs acp smoke codex --prompt "Say hello"
```

The smoke command initializes the agent, creates a temporary external ACP
session for the selected workspace, sends one prompt, prints streamed events,
and exits. If an agent requests permission during a smoke run, the request is
printed and cancelled.

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

## Troubleshooting

`ACP agent '<name>' is not configured`: add `[acp_agents.<name>]` to the
effective config, or check that project config did not override the global
entry.

`ACP agent '<name>' is disabled`: set `enabled = true` or choose another agent.

`command is required`: set `command` in the agent table. `args` cannot replace
`command`.

Spawn or command-not-found errors: use an absolute executable path or ensure the
command is on the `PATH` used to launch `thndrs`.

Authentication failures: run the agent's own login command if it has one, then
retry. `thndrs` calls agent-owned ACP auth methods but does not store tokens,
cookies, refresh state, or client-owned credentials.

Initialize, session, or prompt timeouts: increase `timeout_secs` for slow
startup or long prompts, and check the agent's stderr output.

Protocol parse errors or hangs: the ACP child process must keep stdout
protocol-clean. Logs, banners, progress text, and warnings should go to stderr.

Unsupported protocol version: the current client path expects ACP v1.

Unsupported terminal requests should not occur with current `thndrs`, because
terminal capability is advertised and implemented. If an agent still fails
terminal calls, inspect the command, cwd, and output limits in the status rows.

Unsupported remote transport requests require a stdio bridge today. Remote and
custom ACP transports are intentionally not configurable until a concrete agent
or deployment needs them.
