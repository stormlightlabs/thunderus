# MCP Tasks

## EXT-1: Apply project trust and permissions to MCP

- [x] Keep project MCP configuration inactive until explicitly trusted for
      MCP.
- [x] Make trust decisions scoped, durable, inspectable, and revocable.
- [x] Route server startup and MCP capability use through the shared trust and
      permission flow. Resource access will enter through this boundary in
      EXT-2.
- [x] Record server, capability, requested authority, decision, result, and
      observed effects where available.
- [x] Make configuration precedence visible and state when a server runs
      outside an enforcing sandbox.

## EXT-2: Add bounded MCP resources

**Blocked by:** EXT-1.

- [x] List compact namespaced metadata only when a server advertises resources.
- [x] Fetch contents explicitly with URI, item, byte, timeout, and
      serialization limits.
- [x] Preserve media type and distinguish text from opaque binary data.
- [x] Apply trust, permission, cancellation, redaction, and auditing.
- [x] Isolate resource failures from unrelated servers and tools.

## EXT-3: Make MCP lifecycle and failures diagnosable

**Blocked by:** EXT-1.

- [x] Use disabled, blocked by trust, starting, ready, degraded, failed, and
      stopped consistently across commands and status surfaces.
- [x] Identify configuration scope and failure phase.
- [x] Bound and redact stderr and protocol diagnostics.
- [x] Isolate failed servers and settle their processes during cancellation and
      shutdown.
- [x] Recommend only actions supported by the current CLI.

## EXT-7: Show blocked MCP configuration in the TUI

**What to build:** Make `/mcp` report every configured server that matters to
the current workspace, including project definitions blocked by absent or stale
trust. The blocked state should lead with the server and the action that makes
it available.

**Blocked by:** EXT-1.

**Acceptance criteria:**

- [ ] `/mcp` distinguishes active, disabled, and blocked project definitions
      with text labels, configuration scope, and transport.
- [ ] A project file containing only blocked servers does not produce `no MCP
    servers configured`.
- [ ] A blocked definition that would replace a global definition says so.
- [ ] Blocked output gives the supported recovery command without exposing
      headers, environment values, or other secrets.
- [ ] CLI and TUI status rows come from one semantic projection of effective MCP
      state rather than separate state rules.

**Verification:**

- Focused tests for empty, global-only, trusted project, untrusted project,
  stale project, and project-overrides-global states.
- Render and inspect `/mcp` output at the existing normal, narrow, and tiny test
  widths. Check wrapping, state labels, and no-color legibility.

## EXT-8: Manage project MCP trust in the TUI

**What to build:** Let users inspect, grant, and revoke project MCP trust from a
focused TUI surface while keeping the exact-hash trust behavior from EXT-1.

**Blocked by:** EXT-7.

**Acceptance criteria:**

- [ ] `/mcp trust` shows the workspace, project config path and hash, server
      names and transports, global definitions that would be replaced, and the
      process-permission containment warning before approval.
- [ ] Approval trusts only the hash shown in the surface and reloads effective
      MCP configuration without eagerly starting servers.
- [ ] Editing the project file after approval returns the TUI to a blocked,
      stale state.
- [ ] `/mcp revoke` asks for confirmation when revocation would deactivate
      project definitions and reports the resulting state.
- [ ] `Enter` confirms the selected action, `Esc` cancels, and both paths restore
      composer focus without losing the draft.
- [ ] The surface remains usable at narrow and tiny terminal sizes, and focus or
      selection does not depend on color.

**Verification:**

- Focused state-transition tests for approval, cancellation, revocation, stale
  hashes, reload failure, draft retention, and focus restoration.
- Ratatui buffer or snapshot coverage for the decision, warning, success, and
  failure states at representative widths. Inspect changed snapshots cell by
  cell.
- Exercise the complete trust and revoke flow in a real terminal.

## EXT-9: Add guided MCP configuration

**What to build:** Add CLI commands that create, update, and remove one MCP
server definition without requiring users to write TOML. This configures a
connection; it does not download, install, or start an MCP server.

**Blocked by:** EXT-1.

**Acceptance criteria:**

- [ ] The add command requires an explicit global or project scope and accepts
      either a stdio command with arguments or a Streamable HTTP URL.
- [ ] The remove command requires the same explicit scope and names the server
      definition it will remove.
- [ ] Writes preserve unrelated server definitions and comments, validate the
      complete result, and replace the destination file atomically.
- [ ] Commands reject invalid names, conflicting transport options, and secret
      values supplied directly through command-line flags.
- [ ] Adding project configuration does not grant trust. Output names the file
      changed and tells the user to review it, inspect status, and trust it.
- [ ] Configuration commands never run package-manager commands or connect to
      the server as a side effect.
- [ ] Public MCP usage and CLI documentation show the guided flow while keeping
      manual TOML configuration available.

**Verification:**

- Focused command tests for both transports and scopes, replacement of an
  existing name, removal, malformed existing TOML, comment preservation,
  validation failure, and interrupted writes.
- `cargo test -p thndrs mcp` or the narrowest matching command-test filter.
- `pnpm --dir docs build` after updating the public documentation.

## EXT-10: Design trusted MCP package distribution

**What to build:** Decide whether thndrs should discover, install, upgrade, or
remove MCP server packages. Produce an approved security and portability design
before implementation tickets are added.

**Blocked by:** None - can start immediately.

**Acceptance criteria:**

- [ ] The design names supported package sources and package managers, or
      explains why installation remains external to thndrs.
- [ ] It treats the official MCP Registry as preview discovery metadata, not an
      artifact host or a catalog that host applications should consume
      directly, unless the upstream model changes.
- [ ] It defines provenance, integrity verification, version pinning, update and
      removal behavior, install scope, and user approval before commands run.
- [ ] It separates package installation authority from project MCP trust and
      tool-call permissions.
- [ ] It covers offline behavior, cross-platform launch commands, secret
      handling, audit records, and recovery from partial installation.
- [ ] It defines what a registry or catalog entry may assert and what thndrs
      verifies independently, including artifact identity and hashes where the
      package format provides them.
- [ ] Follow-up implementation work is split into independently verifiable
      tickets after the design is approved.

**Verification:**

- Review the design against at least the MCP Registry model and the package
  managers proposed for support.
- Threat-model a malicious catalog entry, a compromised package version, a
  changed project config, and an interrupted install or upgrade.

## EXT-11: Move web search to MCP

**What to build:** Remove application-owned web search and use configured MCP
servers for search. Keep `read_url` available for public URLs supplied by the
user, the workspace, or an MCP search result.

**Blocked by:** EXT-1.

**Acceptance criteria:**

- [x] The built-in tool catalog no longer advertises or dispatches
      `web_search`, and the DuckDuckGo and SearXNG search implementations are
      removed.
- [x] The web-search backend selector and URL are removed from CLI flags,
      layered configuration, environment handling, prompts, runtime state,
      diagnostics, and ACP configuration options.
- [x] Existing session records containing historical web-search metadata still
      decode and can be inspected or resumed without enabling built-in search.
- [x] `read_url` retains public-network validation, redirect checks, response
      limits, readable extraction, cancellation, and audit behavior.
- [x] Namespaced MCP tools whose original names represent web search or fetch
      use the search or fetch transcript presentation instead of the generic MCP
      presentation. Other MCP tools remain generic.
- [x] With no search MCP server configured, the model is not offered a search
      tool and thndrs does not imply that search is available.
- [x] Public MCP documentation explains that web search requires a configured
      server, uses `xngmcp` as one example, and does not require its package or
      tool names.

**Verification:**

- Focused tool-catalog, configuration, session compatibility, prompt, ACP, and
  transcript classification tests.
- Exercise one configured search MCP server from discovery through a tool call,
  then pass a returned URL to built-in `read_url`.
