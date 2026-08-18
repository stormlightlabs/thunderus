---
title: "MCP"
---

`thndrs` connects to Model Context Protocol servers and exposes their tools to
the model. It supports local stdio servers and remote Streamable HTTP servers.

## Catalog Discovery

`thndrs mcp catalog` searches server metadata before any server is configured.
The built-in source is the official MCP Registry at
`https://registry.modelcontextprotocol.io`. It is enabled by default and shown
as **preview; uncurated**. Registry entries can name a publisher, package,
version, digest, platform constraint, or curation label, but those are catalog
claims. They are not a thndrs security verdict.

```sh
thndrs mcp catalog search filesystem
thndrs mcp catalog show io.modelcontextprotocol/filesystem
```

Search results identify their source, publisher claim, version, transports, and
curation claim. `show` also prints the package origin, supplied digest, and
platform constraints. These commands only retrieve metadata. They do not
install a package, configure an MCP server, start a process, or contact a
server endpoint.

Catalog sources are global. Project files cannot add, replace, or select a
catalog endpoint. List or change sources with:

```sh
thndrs mcp catalog list
thndrs mcp catalog disable official
thndrs mcp catalog add community https://catalog.example --curation 'community review'
thndrs mcp catalog remove community
```

Custom sources must use an HTTPS base URL and the MCP Registry-compatible
`/v0.1/servers` API. Their optional `--curation` text is displayed as a claim.
Re-enable the built-in source with `thndrs mcp catalog enable official`.

For each source, thndrs stores at most 200 display-safe entries from successful
responses in `~/.thndrs/mcp-catalog-cache/`. Use `--offline` to search that
last snapshot or inspect a cached entry. Output names its retrieval time.
An unavailable or malformed source is reported as a diagnostic and does not
hide results from another source or change configured MCP servers.

Catalog discovery is separate from configuration, project trust, server
startup, and tool permissions. Review the publisher's source and package
origin before using catalog metadata to configure a server.

## Configure from a Catalog

Use `configure` to turn one catalog entry into a local definition. Select the
catalog source, destination scope, local name, and transport. The first command
prints the complete recipe and changes no files:

```sh
thndrs mcp catalog configure io.example/weather \
  --source community --name weather --scope project --transport stdio
```

The preview identifies the catalog and publisher claim, artifact registry or
remote host, exact package version, supplied digest, command or URL,
environment-variable names, and destination path. Re-run the same command with
`--yes` after review. Project definitions still need `thndrs mcp trust`.

For stdio entries, thndrs supports exact npm, PyPI, NuGet, and OCI recipes with
`npx`, `uvx`, `dnx`, and `docker`. It rejects `latest`, ranges, unversioned
packages, incompatible platforms, ambiguous variants, secret arguments, and
recipes that need interactive values. Package runners can download code later
when MCP starts; this command never invokes them. Streamable HTTP recipes must
have one concrete URL and no catalog-supplied header values.

The generated definition records catalog provenance beside the server:
metadata source and retrieval time, entry and package version, origin, supplied
digest, selected package identifier, and the generated transport fingerprint. A
supplied digest is a catalog assertion unless the launcher itself enforces an
image digest. thndrs does not download an artifact or claim to verify one.

## Inspect and Update a Catalog Definition

Inspect the stored provenance and compare its generated transport with the
current definition before changing a catalog-derived server:

```sh
thndrs mcp catalog inspect weather --scope project
```

If the transport fields were edited manually, inspection shows both versions and
marks the recorded projection as historical rather than current catalog
provenance. Inspection works offline because it reads only the MCP file.

Resolve a replacement from the same stored catalog and entry with `update`.
The first command shows the stored and replacement source, version, digest,
origin, command or endpoint, environment-variable names, and transport
configuration; it does not change files.

```sh
thndrs mcp catalog update weather --scope project
thndrs mcp catalog update weather --scope project --yes
```

Use `--version <exact-version>` to review a particular catalog metadata record.
For a package variant that is no longer unambiguous, use `--package
<identifier>`. `--offline` resolves only cached metadata and clearly cannot
establish that a newer recipe is available. Approval atomically replaces the
definition and provenance. A project replacement changes the configuration hash,
so it requires a new `thndrs mcp trust` decision before activation.

Updates and removal only edit thndrs configuration. They never run a server or
package manager, install or uninstall packages, or clear package and container
caches.

## Add a Server

`thndrs` does not provide an MCP package installer. Install a local server using
the command supplied by its publisher, or get the URL and credentials for a
hosted server. Then add its launch command or URL with `mcp add`. The command
writes configuration only. It does not install, start, or contact the server.

Choose a scope every time:

```sh
# A local stdio server available in every workspace.
thndrs mcp add docs --scope global --command npx --arg -y --arg @vendor/server

# A hosted Streamable HTTP server for the current workspace.
thndrs mcp add search --scope project --url https://mcp.example.test/mcp
```

Use `--arg` once for each stdio argument. `mcp add` accepts either `--command`
with optional `--arg` values or `--url`, never both. It does not accept header,
environment, or token flags, so credentials do not enter shell history. Remove
a definition with the same scope:

```sh
thndrs mcp remove docs --scope global
```

The commands preserve unrelated definitions and comments, validate the whole
file, then replace it atomically. A project addition does not grant trust. The
command prints the changed path and the next `mcp status` and `mcp trust`
commands to run after review.

Manual TOML configuration remains available. The guided commands use these
files:

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
