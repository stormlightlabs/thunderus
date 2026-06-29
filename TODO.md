# TODO

## Completed

### fake/v0 harness proof of concept

- Added the Clap entrypoint, Ratatui shell, prompt/transcript flow, deterministic
  fake agent stream, reasoning/tool transcript entries, narrow-layout handling, unit
  tests, snapshots, and `cargo check` coverage.

### Phase 5: Context and read-only tool boundary

- Added visible root `AGENTS.md` context loading, structured request/tool output
  shapes, bounded read-only repository tools backed by `fd`, `rg --json`, and
  Rust-native file range reads, workspace containment, output caps, transcript
  rendering, fixture/unit coverage, and tool-entry snapshots.

## alpha: Usable Coding Assistant

### Phase 6: Umans Provider

- [x] Create `src/providers/{mod,umans}.rs`.
- [x] Read `UMANS_API_KEY` from the environment.
- [x] Store Umans base URL as `https://api.code.umans.ai`.
- [x] Implement `GET /v1/models/info` metadata fetch.
- [x] Parse model capabilities for `umans-coder` and `umans-glm-5.2`.
- [x] Default selected model to `umans-coder`.
- [x] Add a simple model switch path for `umans-glm-5.2`.
- [x] Implement Anthropic-compatible `POST /v1/messages`.
- [x] Send `x-api-key` and `anthropic-version: 2023-06-01`.
- [x] Stream response events into `AgentEvent`.
- [x] Render reasoning/thinking deltas separately from final assistant text if the API
      emits them.
- [x] Convert provider HTTP/auth/rate-limit/stream errors into transcript errors.
- [x] Add a no-network unit test for request construction.
- [x] Add a no-network unit test for stream event parsing using fixtures.
- [x] Add a no-network unit test for model metadata parsing.
- [x] Add an ignored/manual live smoke test gated on `UMANS_API_KEY`.

### Phase 7: Agent Event Loop and Tool UI

- [x] Define one agent loop path for fake and Umans runs.
- [x] Convert provider stream deltas, reasoning deltas, tool-use requests, tool
      results, errors, cancellation, and done states into `AgentEvent`.
- [x] Dispatch Phase 5 read-only tools from provider tool-use requests.
- [x] Append tool results to the transcript before continuing the provider turn.
- [x] Prevent recursive or unbounded tool-call loops with a small per-turn cap.
- [x] Keep tool execution async/non-blocking relative to TUI input and redraw.
- [x] Add a cancel path that stops the provider stream or active tool call and
      returns to an editable prompt.
- [ ] Render an agent status line for idle, sending, thinking, streaming, running
      tool, cancelled, failed, and done states.
- [ ] Render tool calls as first-class transcript entries with name, arguments
      summary, status, duration, truncation state, and error text.
- [ ] Keep reasoning, assistant text, context sources, and tool output visually
      distinct in the transcript.
- [ ] Align the shell with Gridland's AI chat pattern: fixed session sidebar,
      vertical transcript, pinned prompt, and stable footer/status line.
- [ ] Add prompt UI states for editable, submitted, streaming, stopped, and
      errored runs.
- [ ] Show active model, search mode, and workspace path in stable chrome without
      crowding narrow terminals.
- [ ] Add a small `thndrs` banner component using `figlet-rs` and a committed
      `src/fonts/*.flf` font loaded with `include_str!`.
- [ ] Fall back to plain title text when the selected FIGlet font cannot parse or
      the terminal is too narrow.
- [ ] Preserve keyboard behavior while the agent is running: scroll transcript,
      cancel, and quit remain available.
- [ ] Unit-test provider-to-agent event conversion.
- [ ] Unit-test tool dispatch success, failure, cancellation, and loop-cap behavior.
- [ ] Add snapshots for thinking, streaming, running-tool, tool-success,
      tool-failure, cancelled, and provider-error UI states.
- [ ] Add snapshots for normal-width and narrow-width banner/header rendering.

### Phase 8: Search and Extraction

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

### Phase 9: Session Persistence

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

### Phase 10: Safe File Operations

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

### Phase 11: Config, Inspect, and Export

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

### Phase 12: v1 Release Hardening

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
