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

Made `/mcp` report every configured server that matters to
the current workspace, including project definitions blocked by absent or stale
trust. The blocked state should lead with the server and the action that makes
it available.

## EXT-8: Manage project MCP trust in the TUI

Lets users inspect, grant, and revoke project MCP trust from a
focused TUI surface while keeping the exact-hash trust behavior from EXT-1.

## EXT-9: Add guided MCP configuration

Adds CLI commands that create, update, and remove one MCP
server definition without requiring users to write TOML. This configures a
connection; it does not download, install, or start an MCP server.

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

- [x] Resolution selects only a transport and package variant compatible with
      the current platform, or reports why no variant can run.
- [x] Local package recipes contain an exact version and reject `latest` or an
      unversioned package. The preview states when the launcher may download
      code during later MCP startup.
- [x] Before approval, the command shows the catalog, claimed publisher,
      artifact registry or remote host, exact version, supplied digest, complete
      command or URL, environment variable names, destination scope, and path.
- [x] Approval writes the server definition and its catalog provenance through
      the validated atomic configuration path. Cancellation changes no files.
- [x] Catalog metadata cannot supply literal secret values, hidden command
      arguments, catalog endpoints, project trust, startup approval, or
      tool-call permission.
- [x] Recorded hashes and identity labels distinguish catalog assertions from
      values enforced by the selected launcher. thndrs does not claim to verify
      an artifact it did not download.
- [x] Configuration does not contact the remote server, execute the launch
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

- [x] Inspection reports the stored catalog URL, entry identity and metadata
      version, retrieval time, package or remote origin, exact version, supplied
      digest, and generated transport configuration.
- [x] A manual change to generated transport fields is visible and prevents
      thndrs from presenting the old projection as current catalog provenance.
- [x] Update resolves one new exact recipe and shows source, version, digest,
      command, environment-name, and transport changes before approval.
- [x] Cancellation and resolution failure preserve the current definition;
      approval uses validated atomic replacement and causes project trust to
      become stale when the project configuration hash changes.
- [x] Update never executes the server or package manager. Removal deletes the
      definition and its provenance without uninstalling packages or clearing
      external caches.
- [x] Offline mode can inspect stored provenance and configuration but cannot
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

Removed application-owned web search and use configured MCP
servers for search. Keep `read_url` available for public URLs supplied by the
user, the workspace, or an MCP search result.
