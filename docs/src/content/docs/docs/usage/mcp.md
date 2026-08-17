---
title: "MCP"
---

`thndrs` connects to Model Context Protocol servers and exposes their tools to
the model. It supports local stdio servers and remote Streamable HTTP servers.

## Add a Server

`thndrs` does not provide an MCP package installer. Install a local server using
the command supplied by its publisher, or get the URL and credentials for a
hosted server. Then add the launch command or URL to an MCP config file. The
configuration step does not launch the server; a package runner such as `npx`
may download or cache it later when `thndrs` starts that command.

- Global: `~/.thndrs/mcp.toml`
- Project: `.thndrs/mcp.toml`

Global servers are available in every workspace. Project servers apply only to
that workspace and require a trust decision before `thndrs` starts them.

For a local server, split the publisher's launch command into `command` and
`args`. For example, a command written as `npx -y @vendor/server` becomes:

```toml
[servers.docs]
transport = "stdio"
command = "npx"
args = ["-y", "@vendor/server"]
enabled = true
timeout_secs = 20
```

`command` may also name an installed executable or a package runner such as
`uvx` or `docker`. `thndrs` passes `args` directly and does not parse a shell
command string.

For a hosted server, configure its endpoint and any required headers:

```toml
[servers.search]
transport = "streamable_http"
url = "https://mcp.example.test/mcp"
headers = { authorization = "Bearer ${THNDRS_MCP_TOKEN}" }
timeout_secs = 20
```

Server names may contain ASCII letters, digits, `_`, and `-`.

## Trust Project Configuration

A project `.thndrs/mcp.toml` file is inactive until you trust its current
contents from that workspace:

```sh
thndrs mcp status
thndrs mcp list
thndrs mcp trust
thndrs mcp test docs
```

`mcp list` shows blocked project servers, their configuration source, and
whether a project definition would replace a global server with the same name.
Review the project file before running `mcp trust`.

Trust applies to the workspace and the file's exact SHA-256 hash. Editing the
file blocks its servers until you review and trust the new version. Remove the
decision with:

```sh
thndrs mcp revoke
```

Global MCP configuration does not use this project trust step because it is
written directly to the user's `~/.thndrs` directory.

## Configuration

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
args = ["./tools/files-mcp-server.js", "${PROJECT_ROOT}"]
env = { NODE_ENV = "production" }
timeout_secs = 10
```

Use absolute paths or commands available on the `PATH` used to launch
`thndrs`. The child process inherits the current process environment plus the
configured `env` values. Values such as `${PROJECT_ROOT}` only work when that
variable exists in the environment that launches `thndrs`.

## Web search

Web search requires a configured MCP server. `thndrs` has no built-in search
backend, so a workspace with no search-capable server can only read public URLs
it already has. [xngmcp](https://github.com/stormlightlabs/xngmcp) is one
example; any MCP search server can work, and no server package or tool name is
required.

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

## Resources

Resources are not added to the model context during startup. List the compact,
namespaced metadata only when the server advertises resource support:

```sh
thndrs mcp resources docs
```

Each row names the `mcp__docs__resource` namespace, resource name, URI, media
type, and reported size. Fetch one URI explicitly:

```sh
thndrs mcp resource docs 'memo://status'
```

The read result is bounded JSON. It preserves the returned URI and media type,
labels text as `text` and base64 data as `opaque_binary`, and marks omitted
items or data as truncated. Reads accept at most eight content items and 128
KiB of serialized data; the server's configured timeout still applies.

## Diagnostics

MCP commands surface configuration and server failures as diagnostics instead
of hiding them in provider output.

```sh
thndrs mcp list
thndrs mcp status
thndrs mcp test docs
thndrs mcp tools docs
```

`mcp list` prints each configured or trust-blocked server, its source, status,
and transport. A configured server is `stopped` until a command or agent run
starts it; lifecycle labels are `disabled`, `blocked by trust`, `starting`,
`ready`, `degraded`, `failed`, and `stopped`. Active entries also state that no
enforcing sandbox is present. `mcp status` prints the project trust state and
current configuration hash. `mcp test` initializes one server and prints
readiness plus the tool count. `mcp tools` prints provider-visible tool names
and descriptions.

Startup diagnostics identify the failed phase, including skipped servers with
unresolved environment variables, initialize failures, `tools/list` failures,
`resources/list` and `resources/read` failures, protocol-version mismatch
notices, and bounded, redacted stderr captured from stdio servers.

## Security Limits

MCP servers are not a sandbox.

A stdio MCP server is a local process running as the same user as `thndrs`.

A Streamable HTTP server receives the arguments sent to it over the configured
endpoint and headers.

MCP configuration cannot rewrite built-in tool schemas, prompt identity, or
local tool policy.

Agent-initiated MCP calls, including the `mcp__{server}__resource_read` tool
available only from servers that advertise resources, use the shared tool
permission and execution path. Calls are timed out, output is capped,
deterministic redaction is applied, and session records identify the server,
capability, requested authority, decision, and result. Running `thndrs mcp
call` or `mcp resource` is a direct user action and calls the server without an
additional prompt.

Only configure MCP servers that you are willing to let the model call.

Use an OS-level sandbox, container, or restricted credentials when the server
can touch files, networks, databases, browsers, or other local state.
