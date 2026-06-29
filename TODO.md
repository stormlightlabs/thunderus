# TODO

## Completed

### fake/v0 harness proof of concept

- Added the Clap entrypoint, Ratatui shell, prompt/transcript flow, deterministic
  fake agent stream, reasoning/tool transcript entries, narrow-layout handling, unit
  tests, snapshots, and `cargo check` coverage.

### Context and read-only tool boundary

- Added visible root `AGENTS.md` context loading, structured request/tool output
  shapes, bounded read-only repository tools backed by `fd`, `rg --json`, and
  Rust-native file range reads, workspace containment, output caps, transcript
  rendering, fixture/unit coverage, and tool-entry snapshots.

### Umans Provider

- Added the concrete Umans provider client, model metadata support for
  `umans-coder` and `umans-glm-5.2`, Anthropic-compatible message requests,
  streaming `AgentEvent` conversion, provider error mapping, no-network fixture
  tests, and a gated live smoke test.

### Agent Event Loop and Tool UI

- Added the shared fake/Umans agent loop, bounded tool-call dispatch,
  cancellation, non-blocking UI updates, Gridland-style sidebar/transcript/prompt
  layout, model/search/workspace chrome, `figlet-rs` banner fallback, event-loop
  unit coverage, and snapshots for streaming, tools, errors, cancellation, and
  banner states.

## alpha: Usable Coding Assistant

### Phase 8: Search and Extraction

- [x] Add search mode config: `native`, `exa`, `none`.
- [x] Default search mode to `native`.
- [x] Send `X-Umans-Websearch-Provider: native` when search is enabled.
- [x] Allow `X-Umans-Websearch-Provider: exa` for manual experiments.
- [x] Allow `X-Umans-Websearch-Provider: none` to pass a local `web_search` tool
      through unchanged.
- [x] Represent server-side search as transcript tool events.
- [x] Verify a search-using prompt still returns normal assistant text when search is
      disabled.
- [x] Unit-test header selection for `native`, `exa`, and `none`.
- [x] Add snapshots for search-started, search-result, and search-error transcript
      states.
- [ ] Inspect Lectito's crate/CLI API before adding any dependency.
- [ ] Prefer a path dependency on the local `lectito` crate if its public API is stable
      enough.
- [ ] Reuse Lectito extraction for already-fetched HTML.
- [ ] Port Lectito MCP's DuckDuckGo HTML search only if provider-native search is
      insufficient.
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

### Phase 9: Prompt Assembly and Context Contract

- [ ] Define a structured `PromptBundle` with base identity, harness policy,
      environment metadata, project context, tool catalog, transcript tail, and
      current user turn.
- [ ] Keep base prompt text short and specific to `thndrs` instead of copying a
      larger agent prompt wholesale.
- [ ] Lower `PromptBundle` into Umans Anthropic-compatible messages.
- [ ] Include loaded `AGENTS.md` context below harness policy and direct user
      instructions.
- [ ] Include read-only tool names, input schemas, limits, and truncation behavior
      in the tool catalog.
- [ ] Preserve a model-visible transcript tail that excludes UI-only status
      entries.
- [ ] Record prompt metadata in session JSONL later without storing full raw
      provider payloads by default.
- [ ] Unit-test prompt-bundle ordering and precedence.
- [ ] Unit-test Umans message lowering with fixture context and transcript tails.
- [ ] Add snapshots for prompt debug/inspect output if a debug view exists.

### Phase 10: Session Persistence

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

### Phase 11: Safe File Operations

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

### Phase 12: Config, Inspect, and Export

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

### Phase 13: v1 Release Hardening

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
