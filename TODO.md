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

### Search and Extraction

- Added native/Exa/disabled Umans web-search modes, transcript rendering for
  search events, Lectito-backed extraction and DuckDuckGo fallback plumbing,
  public-URL safety limits, private-network rejection, truncation metadata, local
  fixture tests, and snapshots for search success and failure states.

## alpha: Usable Coding Assistant

### Phase 9: Prompt Assembly and Context Contract

- [x] Define a structured `PromptBundle` with base identity, harness policy,
      environment metadata, project context, tool catalog, transcript tail, and
      current user turn.
- [x] Keep base prompt text short and specific to `thndrs` instead of copying a
      larger agent prompt wholesale.
- [x] Round current date/time in prompt context for cache stability.
- [x] Lower `PromptBundle` into Umans Anthropic-compatible messages.
- [x] Add `--print-prompt` to print the assembled prompt bundle/lowered messages
      with secrets redacted and no provider call.
- [x] Include loaded `AGENTS.md` context below harness policy and direct user
      instructions.
- [x] Use Umans/provider-native tool schemas for local tools.
- [ ] Keep text tool descriptions minimal: name, purpose, safety limits, and
      truncation behavior.
- [ ] Send the compact, stably ordered tool schema every provider turn unless
      reusable-history or prompt-cache support is explicit.
- [ ] Preserve a projected model-visible transcript tail that excludes UI-only
      status entries, live-only stream deltas, sidebar state, and renderer
      artifacts.
- [ ] Include full `AGENTS.md` text only when its content hash changes and the
      provider supports history reuse.
- [ ] Fall back to active size-capped `AGENTS.md` context when history reuse is
      unavailable.
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
