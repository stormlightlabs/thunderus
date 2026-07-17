---
title: "MCP"
---

`thndrs` can connect to configured Model Context Protocol servers and expose
their tools to the model. MCP tools are discovered at startup, added to the
same provider tool catalog as built-in tools, and recorded in the same session
log path.

The protocol and transport layer uses the official Rust MCP SDK, `rmcp`.
`thndrs` keeps local policy outside the SDK: config loading, tool namespacing,
timeouts, output caps, redaction, and session audit records stay in the local
tool manager.

## Config Files

MCP servers are configured separately from ordinary `thndrs` settings:

- Global: `~/.thndrs/mcp.toml`
- Project: `.thndrs/mcp.toml`

Project server definitions override global server definitions with the same
name. Server names may contain ASCII letters, digits, `_`, and `-`.

```toml
[servers.docs]
transport = "stdio"
command = "docs-mcp"
args = ["--workspace", "${THNDRS_WORKSPACE}"]
enabled = true
timeout_secs = 20
```

```toml
[servers.search]
transport = "streamable_http"
url = "https://mcp.example.test/mcp"
headers = { authorization = "Bearer ${THNDRS_MCP_TOKEN}" }
timeout_secs = 20
```

Supported keys:

- `transport`: `stdio` or `streamable_http`; defaults to `stdio`.
- `command`: executable for `stdio` servers.
- `args`: argv entries after `command`.
- `env`: environment variables passed to a `stdio` child process.
- `url`: endpoint for `streamable_http` servers.
- `headers`: request headers for `streamable_http` servers.
- `enabled`: whether the server is discoverable and callable; defaults to
  `true`.
- `timeout_secs`: startup, `tools/list`, and `tools/call` timeout; defaults to
  `20`.

Environment expansion uses `${NAME}` inside values. If a variable is missing,
that server is skipped and a diagnostic is recorded. Secret values in `env` and
`headers` are redacted in config metadata and diagnostics.

## Stdio Servers

A stdio server is a local subprocess. `command` is the executable and `args` is
passed as an argv array; `thndrs` does not run a shell unless the executable is
itself a shell.

```toml
[servers.files]
command = "node"
args = ["./tools/files-mcp-server.js", "${THNDRS_WORKSPACE}"]
env = { NODE_ENV = "production" }
timeout_secs = 10
```

Use absolute paths or commands available on the `PATH` used to launch
`thndrs`. The child process inherits the current process environment plus the
configured `env` values.

## Tool Names

Provider-visible MCP tool names are always namespaced:

```text
mcp__{server}__{tool}
```

For a server named `docs` exposing an original MCP tool named `search`, the
provider sees `mcp__docs__search`. The CLI `mcp call` command takes the
original MCP tool name, not the namespaced provider name:

```sh
thndrs mcp call docs search --json '{"query":"config"}'
```

Session records keep both the configured server name and the original MCP tool
name so exports remain inspectable.

## Diagnostics

MCP commands surface configuration and server failures as diagnostics instead
of hiding them in provider output.

```sh
thndrs mcp list
thndrs mcp test docs
thndrs mcp tools docs
```

`mcp list` prints each configured server, enabled status, and transport.
`mcp test` initializes one server and prints readiness plus the tool count.
`mcp tools` prints provider-visible tool names and descriptions.

Startup diagnostics include skipped servers with unresolved environment
variables, disabled servers, initialize failures, `tools/list` failures,
protocol-version mismatch notices, and bounded stderr captured from stdio
servers.

## Security Limits

MCP servers are not a sandbox.

A stdio MCP server is a local process running as the same user as `thndrs`.

A Streamable HTTP server receives the arguments sent to it over the configured
endpoint and headers.

MCP configuration cannot rewrite built-in tool schemas, prompt identity, or
local tool policy.

MCP tools still use the shared execution path, so calls are timed out,
output is capped, deterministic redaction is applied, failures are
stable tool failures, and session JSONL gets normal `tool_started` and
`tool_finished` records.

Only configure MCP servers that you are willing to let the model call.

Use an OS-level sandbox, container, or restricted credentials when the server
can touch files, networks, databases, browsers, or other local state.
