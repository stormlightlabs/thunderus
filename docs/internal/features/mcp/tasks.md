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

- [x] `/mcp` distinguishes active, disabled, and blocked project definitions
      with text labels, configuration scope, and transport.
- [x] A project file containing only blocked servers does not produce `no MCP
    servers configured`.
- [x] A blocked definition that would replace a global definition says so.
- [x] Blocked output gives the supported recovery command without exposing
      headers, environment values, or other secrets.
- [x] CLI and TUI status rows come from one semantic projection of effective MCP
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

- [x] `/mcp trust` shows the workspace, project config path and hash, server
      names and transports, global definitions that would be replaced, and the
      process-permission containment warning before approval.
- [x] Approval trusts only the hash shown in the surface and reloads effective
      MCP configuration without eagerly starting servers.
- [x] Editing the project file after approval returns the TUI to a blocked,
      stale state.
- [x] `/mcp revoke` asks for confirmation when revocation would deactivate
      project definitions and reports the resulting state.
- [x] `Enter` confirms the selected action, `Esc` cancels, and both paths restore
      composer focus without losing the draft.
- [x] The surface remains usable at narrow and tiny terminal sizes, and focus or
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

- [x] The add command requires an explicit global or project scope and accepts
      either a stdio command with arguments or a Streamable HTTP URL.
- [x] The remove command requires the same explicit scope and names the server
      definition it will remove.
- [x] Writes preserve unrelated server definitions and comments, validate the
      complete result, and replace the destination file atomically.
- [x] Commands reject invalid names, conflicting transport options, and secret
      values supplied directly through command-line flags.
- [x] Adding project configuration does not grant trust. Output names the file
      changed and tells the user to review it, inspect status, and trust it.
- [x] Configuration commands never run package-manager commands or connect to
      the server as a side effect.
- [x] Public MCP usage and CLI documentation show the guided flow while keeping
      manual TOML configuration available.

**Verification:**

- Focused command tests for both transports and scopes, replacement of an
  existing name, removal, malformed existing TOML, comment preservation,
  validation failure, and interrupted writes.
- `cargo test -p thndrs mcp` or the narrowest matching command-test filter.
- `pnpm --dir docs build` after updating the public documentation.

## EXT-10A: Search configurable MCP catalogs

**What to build:** Let users search and inspect MCP server metadata from a
global set of catalogs. Include the official MCP Registry as a built-in preview,
uncurated source without treating its entries as trusted software.

**Blocked by:** None - can start immediately.

**Acceptance criteria:**

- [x] Search and detail commands identify the source catalog, claimed
      publisher, available transports, package origins, versions, platform
      constraints, and curation claims without launching a server.
- [x] The official registry endpoint is enabled by default, clearly labelled as
      preview and uncurated, and can be disabled.
- [x] Users can add and remove global API-compatible catalog sources; project
      configuration cannot select or replace catalog endpoints.
- [x] Catalog responses are bounded, validated, and isolated so one unavailable
      or malformed source does not hide results from another source.
- [x] A bounded cache supports offline search of the last successful metadata
      snapshot and shows when it was retrieved. Catalog failure never affects
      configured MCP servers.
- [x] Output does not present publisher identity, curation labels, versions, or
      supplied hashes as a thndrs security verdict.
- [x] Internal and public MCP documentation explain catalog configuration,
      preview status, caching, and the discovery-only security boundary.

**Verification:**

- Focused client and command tests with deterministic catalog fixtures for
  search, detail, pagination, multiple sources, invalid data, response limits,
  disabled sources, unavailable sources, and offline cache fallback.
- `cargo test -p thndrs mcp` or the narrowest matching test filters.
- `pnpm --dir docs build` after updating public documentation.

## EXT-10B: Configure a server from catalog metadata

**What to build:** Turn a selected catalog result into an exact local launch
recipe or Streamable HTTP definition through the guided MCP configuration flow.
Show the resolved command, origin, and authority boundary before writing it.

**Blocked by:** EXT-9 and EXT-10A.

**Acceptance criteria:**

- [ ] Resolution selects only a transport and package variant compatible with
      the current platform, or reports why no variant can run.
- [ ] Local package recipes contain an exact version and reject `latest` or an
      unversioned package. The preview states when the launcher may download
      code during later MCP startup.
- [ ] Before approval, the command shows the catalog, claimed publisher,
      artifact registry or remote host, exact version, supplied digest, complete
      command or URL, environment variable names, destination scope, and path.
- [ ] Approval writes the server definition and its catalog provenance through
      the validated atomic configuration path. Cancellation changes no files.
- [ ] Catalog metadata cannot supply literal secret values, hidden command
      arguments, catalog endpoints, project trust, startup approval, or
      tool-call permission.
- [ ] Recorded hashes and identity labels distinguish catalog assertions from
      values enforced by the selected launcher. thndrs does not claim to verify
      an artifact it did not download.
- [ ] Configuration does not contact the remote server, execute the launch
      command, invoke a package manager, or modify an external package cache.

**Verification:**

- Focused resolution and command tests for remote servers, each supported local
  recipe shape, incompatible platforms, ambiguous variants, unpinned versions,
  secret-bearing metadata, approval, cancellation, and interrupted writes.
- Verify that a catalog-derived project definition remains blocked until its
  exact configuration hash is trusted through the existing MCP trust flow.
- `cargo test -p thndrs mcp` or the narrowest matching test filters.

## EXT-10C: Review and update catalog-derived configuration

**What to build:** Let users inspect the provenance of a catalog-derived MCP
definition and replace it with a newer exact recipe after reviewing the full
configuration diff. Keep external package state outside thndrs ownership.

**Blocked by:** EXT-10B.

**Acceptance criteria:**

- [ ] Inspection reports the stored catalog URL, entry identity and metadata
      version, retrieval time, package or remote origin, exact version, supplied
      digest, and generated transport configuration.
- [ ] A manual change to generated transport fields is visible and prevents
      thndrs from presenting the old projection as current catalog provenance.
- [ ] Update resolves one new exact recipe and shows source, version, digest,
      command, environment-name, and transport changes before approval.
- [ ] Cancellation and resolution failure preserve the current definition;
      approval uses validated atomic replacement and causes project trust to
      become stale when the project configuration hash changes.
- [ ] Update never executes the server or package manager. Removal deletes the
      definition and its provenance without uninstalling packages or clearing
      external caches.
- [ ] Offline mode can inspect stored provenance and configuration but cannot
      claim that a newer version is available without current catalog data.

**Verification:**

- Focused tests for unchanged, newer, removed, malformed, and unavailable
  catalog entries; manual configuration drift; approval and cancellation;
  stale project trust; atomic-write failure; and provenance cleanup on removal.
- Confirm that update and removal leave representative npm, Python, container,
  and MCPB caches untouched.
- `cargo test -p thndrs mcp` or the narrowest matching test filters.
- `pnpm --dir docs build` after updating public documentation.

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
