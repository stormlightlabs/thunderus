# MCP

MCP provides typed operations, resources, prompts, discovery, and server
lifecycle. Project trust, authority, containment, redaction, auditing, and
transcript behavior remain shared application policy. Discovering project MCP
configuration never activates it, and server startup or capability use cannot
exceed the current run's authority.

Resources provide structured context without representing every read as a tool
call or injecting server content at startup. Listing stays compact and
namespaced; fetching is explicit and bounded. Every configured server exposes
the same lifecycle vocabulary so failures identify their configuration scope
and phase without exposing secrets.

## Web access

Web search belongs behind MCP. `thndrs` discovers whichever search tools the
user configures instead of selecting and maintaining DuckDuckGo, SearXNG, or
another search backend. Users may configure Brave, Tavily,
[xngmcp](https://github.com/stormlightlabs/xngmcp) backed by a local SearXNG
instance, or another implementation without application-specific integration.
Documentation may use one server as an example, but application code must not
depend on its package or tool names.

Keep `read_url` as a built-in tool. Reading a public URL is useful when the user
or the workspace already supplies the URL, and it gives MCP search tools a
provider-independent fetch path. The application continues to own URL scheme
validation, private-network and redirect rejection, response limits, readable
text extraction, cancellation, and audit behavior for this tool. MCP servers
may also expose their own fetch tools; users can choose either path.

A setup without a search MCP server can read known URLs but cannot discover
pages. Tool discovery and the system prompt must make that state clear rather
than implying that web search is always available. Namespaced MCP search and
fetch tools should retain the transcript presentation used for built-in web
operations when their original tool names identify those operations.

Removing application-owned search also removes its backend selector, backend
URL, CLI flags, configuration keys, environment handling, prompt metadata, and
ACP configuration option. Existing session records that contain web-search
metadata remain readable; the value is historical and does not restore a
built-in search tool.

## Setup and installation

`thndrs` configures MCP connections. The configuration flow does not download
packages, run install scripts, or launch the configured command. A user either
installs a local server through its publisher or package manager and gives
`thndrs` the launch command, or supplies a package runner such as `npx` as that
command. A package runner may download or cache the server later, when normal
MCP startup launches it. Hosted servers need a Streamable HTTP URL instead.

The guided configuration flow writes one server definition to either the
global or project MCP file. Scope is required. Adding project configuration
does not trust it: the command prints the path it changed and the review and
trust commands the user can run next. Configuration writes must preserve
unrelated servers and comments, validate the resulting file before replacing
it, and use an atomic replacement in the destination directory.

Do not accept secret values as command-line arguments. The initial guided flow
may accept environment variable names or leave authentication for manual
editing, but it must not put tokens in shell history or command output.

Package discovery, installation, upgrades, and removal need a separate
supply-chain design. That design must settle provenance, integrity and version
pinning, supported package managers, install authority, and what thndrs can
verify before any command executes.

The reference model does not collapse discovery and installation. The official
MCP Registry is a preview metadata repository that points to packages in npm,
PyPI, NuGet, OCI registries, or MCPB release artifacts; it does not host those
artifacts. Its guidance also directs host applications toward downstream
catalogs rather than direct registry consumption. A future thndrs catalog must
therefore make its source and curation policy explicit and independently verify
the artifact it installs. OpenCode's current flow is a useful configuration
baseline: users add either a local launch command or a remote URL, select
project or global scope, and configure credentials separately.

Reviewed references:

- [MCP Registry overview](https://modelcontextprotocol.io/registry/about)
- [MCP Registry package types](https://modelcontextprotocol.io/registry/package-types)
- [OpenCode MCP server configuration](https://opencode.ai/v2/docs/mcp-servers)

## TUI status and trust

`/mcp` is the TUI status surface. It lists active, disabled, and blocked project
servers together, including configuration scope and transport. A project file
that contains only blocked servers is not an empty configuration. Show the
blocked definitions, then show one warning with the next supported action:
`Run thndrs mcp trust to activate this project configuration.`

Project trust should also be manageable without leaving the TUI. The trust
surface is a focused decision, not another status row. Before approval it shows
the workspace, project config path and hash, server names and transports,
global definitions that would be replaced, and the fact that servers run with
the thndrs process permissions. Approval trusts only the displayed hash. A
changed file requires a new decision.

Keep the composer draft while the trust surface is open. `Enter` confirms the
selected action, `Esc` cancels, and focus returns to the composer after either
outcome. Trusting reloads MCP configuration but does not start a server until a
normal discovery or call path needs it. Revocation requires confirmation when
it would deactivate active project definitions.

Derive CLI and TUI rows from one semantic status projection over
`EffectiveMcpConfig`. State transitions belong in application code; rendering
only lays out semantic rows. Use text labels such as `blocked by trust` and
`disabled` so color is never the only state cue. Keep recovery text next to the
blocked condition and avoid a permanent panel for MCP status.

Verify status projection with focused unit tests. Render the affected transcript
and trust states at normal, narrow, and tiny terminal sizes, inspect the changed
cells and wrapping, then exercise trust, cancellation, stale hashes, and focus
restoration in a real terminal.
