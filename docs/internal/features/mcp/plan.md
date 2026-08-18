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

### Catalog discovery

`thndrs` supports configurable MCP catalogs and guided configuration, but does
not install, upgrade, or remove package-manager artifacts. npm, PyPI, NuGet,
OCI, and MCPB remain external distribution systems. This avoids a second
package manager inside `thndrs` and makes partial package installation,
install-script execution, and package cache recovery the responsibility of the
tool that owns those operations.

The official MCP Registry is the built-in discovery source at
`https://registry.modelcontextprotocol.io`. Label it as preview and uncurated.
It supplies metadata that points to packages or hosted servers; it does not
host artifacts, scan their code, or attest that a server is safe. Users can
disable this source or add API-compatible downstream catalogs. Catalog source
configuration is global: project configuration cannot add a catalog or change
where metadata is resolved.

Catalog availability cannot affect configured servers. Cache bounded search
metadata with its retrieval time so users can search the last successful
snapshot offline. A missing, stale, or malformed catalog entry prevents a new
resolution but does not alter an existing server definition. Search results
identify their catalog and any curation claims that catalog supplies.

Adding a search result resolves it to either a Streamable HTTP endpoint or a
local launch recipe supported on the current platform. Before writing, show:

- the catalog, claimed publisher, and artifact registry or remote host;
- the exact package version, digest when supplied, command, arguments, and
  whether the command may download code when it starts;
- required environment variable names, with no secret values; and
- the destination scope and configuration path.

The user approves that complete projection before `thndrs` writes it. Local
package recipes must use an exact version; `latest` and unversioned recipes are
not generated. A supplied digest is recorded as a catalog assertion unless the
selected launcher can enforce it. TLS validates transport to a hosted server,
not its publisher or behavior. `thndrs` must not claim to have verified an
artifact it did not download.

Catalog provenance is stored with the generated server definition: catalog
URL, entry identity and metadata version, retrieval time, package or remote
origin, exact version, supplied digest, and generated transport configuration.
This source metadata is an audit record, not execution authority. Manual edits
remain possible and invalidate claims that depend on the previous projection.

Package updates are configuration updates. `thndrs` may resolve a newer exact
recipe, show the provenance and command diff, and replace the definition after
approval. It does not run the package manager or clear its cache. Removing a
catalog-derived server removes its definition and provenance only. Interrupted
writes use the same validated atomic replacement as manual guided
configuration, so no package installation needs recovery.

Catalog access, configuration approval, project trust, server startup, and
tool-call permission are separate decisions. Adding a project definition does
not trust it. Adding either scope does not start the server, approve its tools,
or grant authority to a later package download. Normal MCP startup and shared
permission policy continue to enforce those decisions.

Claude Code and Codex use explicit MCP configuration and can distribute MCP
definitions through plugins. VS Code provides a gallery that writes user or
workspace configuration, then asks for server trust separately. These systems
support the same boundary: discovery can propose a connection, while starting
the resulting server remains a distinct decision.

Reviewed references:

- [MCP Registry overview](https://modelcontextprotocol.io/registry/about)
- [MCP Registry aggregator guidance](https://modelcontextprotocol.io/registry/registry-aggregators)
- [MCP Registry package types](https://modelcontextprotocol.io/registry/package-types)
- [Claude Code MCP configuration](https://code.claude.com/docs/en/mcp)
- [Claude Code plugin marketplaces](https://code.claude.com/docs/en/plugin-marketplaces)
- [Codex MCP configuration](https://developers.openai.com/codex/mcp)
- [VS Code MCP server management](https://code.visualstudio.com/docs/agent-customization/mcp-servers)
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
