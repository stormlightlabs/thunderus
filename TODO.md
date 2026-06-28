# TODO

## fake/v0: Harness Proof

### Phase 1: CLI and Ratatui Shell

- [x] Add dependencies: `ratatui`, `crossterm`, and `clap` with `derive`.
- [x] Add dev dependency: `insta`.
- [x] Create `src/cli.rs`.
- [ ] Define `Cli` with `cwd`, `model`, `websearch`, `tick_rate_ms`, and `no_alt_screen`.
- [ ] Define `WebSearchMode` as a Clap `ValueEnum`: `native`, `exa`, `none`.
- [ ] Parse CLI args in `src/main.rs`.
- [ ] `src/app.rs`: `App`, `Mode`, `RunState`, `Entry`, `Msg`, and `update`.
- [ ] `src/ui.rs`: `ViewState`, `compute_view`, and `render`.
- [ ] `src/lib.rs`: terminal setup, draw loop, event polling, and cleanup.
  - [ ] `src/main.rs` just runs
- [ ] Render static sidebar, transcript placeholder, prompt line, and footer.
- [ ] Support clean exit on `q`, `Ctrl+D`, & `Ctrl+C`.
- [ ] Unit-test CLI defaults.
- [ ] Unit-test invalid `--websearch` handling.
- [ ] Add an `80x24` empty-shell `TestBackend` snapshot.
- [ ] Run `cargo check`.

### Phase 2: Prompt and Transcript

- [ ] Handle printable character input.
- [ ] Handle Backspace.
- [ ] Handle Enter submit.
- [ ] Append submitted text as `Entry::User`.
- [ ] Clear input after submit.
- [ ] Implement `/clear`.
- [ ] Implement `/quit`.
- [ ] Render newest transcript entries in available height.
- [ ] Add unit tests for `update` submit, clear, quit, and backspace behavior.
- [ ] Add a snapshot for one submitted prompt in the transcript.

### Phase 3: Fake Agent Stream

- [ ] Create `src/agent.rs`.
- [ ] Define `AgentEvent`.
- [ ] Include `ReasoningDelta` in `AgentEvent` before the real provider lands.
- [ ] Add a deterministic fake response stream.
- [ ] Send fake stream events into the app loop over a channel.
- [ ] Append assistant deltas into one streaming assistant entry.
- [ ] Append reasoning deltas into one separate streaming reasoning entry.
- [ ] Render fake tool start/output/end as one tool entry.
- [ ] Add stop/cancel behavior for an active fake stream.
- [ ] Test `AgentEvent` handling in `update`.
- [ ] Add snapshots for streaming assistant, reasoning, running tool, and finished
      states.

### Phase 4: Layout Hardening

- [ ] Move all rectangle calculation into `compute_view`.
- [ ] Hide sidebar below the chosen narrow-width threshold.
- [ ] Add a normal-width Ratatui `TestBackend` render test.
- [ ] Add a narrow-width Ratatui `TestBackend` render test.
- [ ] Add pure `compute_view` tests for normal, narrow, and tiny terminal rects.
- [ ] Verify prompt/footer text does not overlap at small sizes.

## alpha: Usable Coding Assistant

### Phase 5: Context and Read-Only Tool Boundary

- [ ] Add explicit context loading for root `AGENTS.md`.
- [ ] Add transcript entry showing loaded context sources.
- [ ] Define structured tool output before implementing any write-capable tool.
- [ ] Define a small internal request shape: prompt, transcript tail, context sources,
      selected model, search mode.
- [ ] Implement read-only `list_files`.
- [ ] Implement read-only `read_file`.
- [ ] Implement read-only `grep`.
- [ ] Render context sources before the model response starts.
- [ ] Keep the first client concrete; do not add a generic provider trait yet.
- [ ] Unit-test context-source collection with and without root `AGENTS.md`.
- [ ] Unit-test read-only tool success and failure paths.
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
- [ ] Resume the latest session on startup.
- [ ] Render saved sessions in the sidebar.
- [ ] Unit-test JSONL encode/decode round trips.
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
- [ ] Document session/config compatibility expectations.
- [ ] Unit-test config precedence.
- [ ] Integration-test `--help`.
- [ ] Integration-test inspect/export against fixture sessions.

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
