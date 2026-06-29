# TODO

## Completed

### fake/v0 harness proof of concept

- Added the Clap entrypoint, Ratatui shell, prompt/transcript flow, deterministic
  fake agent stream, reasoning/tool transcript entries, narrow-layout handling, unit
  tests, snapshots, and `cargo check` coverage.

## alpha: Usable Coding Assistant

### Phase 5: Context and Read-Only Tool Boundary

- [x] Add explicit context loading for root `AGENTS.md`.
- [x] Discover workspace root from `--cwd`; prefer git root when available.
- [x] Treat AGENTS.md as guidance only, not permission/config.
- [x] Enforce an AGENTS.md size cap.
- [x] Mark AGENTS.md truncation visibly in context metadata and transcript status.
- [x] Add transcript entry showing loaded context sources.
- [x] Define structured tool output before implementing any write-capable tool.
- [x] Define a small internal request shape: prompt, transcript tail, context sources,
      selected model, search mode.
- [x] Include AGENTS.md path, scope, content hash, and truncation state in context
      metadata.
- [ ] Implement read-only `find_files` backed by `fd` with `find` fallback.
- [ ] Implement read-only `list_searchable_files` backed by `rg --files` or
      `fd --type file` with `grep` or `find` fallbacks, respectively.
- [ ] Implement read-only `search_text` backed by `rg --json`.
- [ ] Implement `read_file_range` in Rust.
- [ ] Define typed tool inputs for pattern, root, globs, extensions, depth, context,
      and max results.
- [ ] Invoke `fd`/`rg` with `std::process::Command` argv arrays, not shell strings.
- [ ] Enforce workspace-root containment after path normalization.
- [ ] Default to respecting ignore rules and skipping hidden files.
- [ ] Keep hidden files, ignored files, symlink following, and unrestricted searches
      opt-in.
- [ ] Enforce timeout, result-count, stdout/stderr byte, and transcript truncation caps.
- [ ] Treat `rg` exit code `1` as no matches.
- [ ] Do not expose `fd --exec`, `fd --exec-batch`, `rg --pre`, arbitrary `sed`,
      arbitrary `awk`, `sed -i`, or `awk system()`.
- [ ] Add optional `summarize_text` only as canned output-only templates if needed.
- [ ] Render context sources before the model response starts.
- [ ] Keep the first client concrete; do not add a generic provider trait yet.
- [ ] Unit-test context-source collection with and without root `AGENTS.md`.
- [ ] Unit-test missing AGENTS.md behavior.
- [ ] Unit-test oversized AGENTS.md truncation.
- [ ] Unit-test AGENTS.md cannot override user prompt or harness policy.
- [ ] Unit-test read-only tool success and failure paths.
- [ ] Unit-test root containment and path normalization.
- [ ] Unit-test `fd` output parsing with fixture output.
- [ ] Unit-test `rg --json` parsing with fixture output.
- [ ] Unit-test `rg` match, no-match, and failure exit handling.
- [ ] Unit-test output truncation.
- [ ] Add snapshots for successful and failed tool entries.

### Phase 6: Umans Provider

- [ ] Create `src/umans.rs`.
- [ ] Read `UMANS_API_KEY` from the environment.
- [ ] Store Umans base URL as `https://api.code.umans.ai`.
- [ ] Implement `GET /v1/models/info` metadata fetch.
- [ ] Parse model capabilities for `umans-coder` and `umans-glm-5.2`.
- [ ] Default selected model to `umans-coder`.
- [ ] Add a simple model switch path for `umans-glm-5.2`.
- [ ] Implement Anthropic-compatible `POST /v1/messages`.
- [ ] Send `x-api-key` and `anthropic-version: 2023-06-01`.
- [ ] Stream response events into `AgentEvent`.
- [ ] Render reasoning/thinking deltas separately from final assistant text if the API
      emits them.
- [ ] Convert provider HTTP/auth/rate-limit/stream errors into transcript errors.
- [ ] Add a no-network unit test for request construction.
- [ ] Add a no-network unit test for stream event parsing using fixtures.
- [ ] Add a no-network unit test for model metadata parsing.
- [ ] Add an ignored/manual live smoke test gated on `UMANS_API_KEY`.

### Phase 7: Search and Extraction

- [ ] Add search mode config: `native`, `exa`, `none`.
- [ ] Default search mode to `native`.
- [ ] Send `X-Umans-Websearch-Provider: native` when search is enabled.
- [ ] Allow `X-Umans-Websearch-Provider: exa` for manual experiments.
- [ ] Allow `X-Umans-Websearch-Provider: none` to pass a local `web_search` tool
      through unchanged.
- [ ] Represent server-side search as transcript tool events.
- [ ] Verify a search-using prompt still returns normal assistant text when search is
      disabled.
- [ ] Unit-test header selection for `native`, `exa`, and `none`.
- [ ] Add snapshots for search-started, search-result, and search-error transcript
      states.
- [ ] Inspect Lectito's crate/CLI API before adding any dependency.
- [ ] Prefer a path dependency on the local `lectito` crate if its public API is stable
      enough.
- [ ] Reuse Lectito extraction for already-fetched HTML.
- [ ] Port or depend on Lectito MCP's DuckDuckGo HTML search only if provider-native
      search is insufficient.
- [ ] Keep local search result limits small.
- [ ] Detect DuckDuckGo bot-challenge pages.
- [ ] Fetch only public `http`/`https` URLs.
- [ ] Reject private-network targets by default.
- [ ] Enforce redirect, timeout, content-type, and response-size limits.
- [ ] Return title, final URL, truncation state, and Markdown/text content.
- [ ] Render Lectito search and extraction through the same tool event path as Umans
      search.
- [ ] Unit-test DuckDuckGo HTML parsing with local fixtures.
- [ ] Unit-test bot-challenge detection.
- [ ] Unit-test private-network URL rejection.
- [ ] Unit-test oversized-document failure.
- [ ] Unit-test Lectito extraction with local HTML fixtures.

### Phase 8: Session Persistence

- [ ] Define the JSONL session record format.
- [ ] Save transcript entries append-only.
- [ ] Save loaded AGENTS.md context metadata: path, scope, content hash, and
      truncation state.
- [ ] Resume the latest session on startup.
- [ ] Render saved sessions in the sidebar.
- [ ] Unit-test JSONL encode/decode round trips.
- [ ] Unit-test AGENTS.md context metadata round trips.
- [ ] Unit-test corrupt-line handling.
- [ ] Unit-test resume ordering.
- [ ] Add a snapshot for sidebar session-list rendering.

### Phase 9: Safe File Operations

- [ ] Define write transcript event shape.
- [ ] Implement create-file operation.
- [ ] Implement exact-range replace operation.
- [ ] Implement unified patch apply operation.
- [ ] Record file path, operation type, and result for every write.
- [ ] Preserve enough before/after metadata for session audit.
- [ ] Keep long-running shell/process execution out of scope.
- [ ] Unit-test create-file success and already-exists failure.
- [ ] Unit-test exact-range replace success and stale-range failure.
- [ ] Unit-test patch apply success and rejected patch failure.
- [ ] Unit-test failed edits leave files unchanged.
- [ ] Add snapshots for write success and write failure transcript entries.

## v1: Supported Release

### Phase 10: Config, Inspect, and Export

- [ ] Define config file path.
- [ ] Define config keys for model, web search mode, session path, and tick rate.
- [ ] Implement config loading.
- [ ] Implement precedence: CLI flags override env vars, env vars override config,
      config overrides built-in defaults.
- [ ] Add non-TUI session inspect/export command after sessions exist.
- [ ] Keep inspect/export output JSON or JSONL.
- [ ] Include loaded AGENTS.md files, scopes, hashes, and truncation state in
      inspect/export output.
- [ ] Document session/config compatibility expectations.
- [ ] Document AGENTS.md precedence: harness policy, user prompt, CLI/config,
      nearest AGENTS.md, broader AGENTS.md, defaults.
- [ ] Document nested AGENTS.md scoping for v1 or mark it explicitly deferred.
- [ ] Unit-test config precedence.
- [ ] Integration-test `--help`.
- [ ] Integration-test inspect/export against fixture sessions.
- [ ] Integration-test inspect/export includes AGENTS.md context metadata.

### Phase 11: v1 Release Hardening

- [ ] Add `CHANGELOG.md` using Keep a Changelog categories.
- [ ] Document install flow.
- [ ] Document config file and env vars.
- [ ] Document session storage and inspect/export.
- [ ] Document Umans provider setup.
- [ ] Document search modes.
- [ ] Document file-operation safety limits.
- [ ] Confirm packaging with `cargo package`.
- [ ] Confirm install path with local `cargo install --path .`.
- [ ] Add CI check plan for format, clippy, tests, and snapshots.
- [ ] Audit snapshots and session fixtures for secrets and machine-specific paths.
- [ ] Add ignored/manual Umans smoke-test instructions.
- [ ] Write release candidate checklist.
