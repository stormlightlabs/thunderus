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

### Prompt Assembly and Context Contract

- Added structured prompt bundles, short thndrs-specific prompt fragments,
  rounded environment metadata, loaded `AGENTS.md` context, provider-native tool
  schemas, model-visible transcript projection, `--print-prompt` redaction, Umans
  message lowering, unit coverage, and prompt snapshot coverage.

### Session Persistence

- Added append-only JSONL session records, persisted `AGENTS.md` context
  metadata, latest-session resume, sidebar session-list rendering, encode/decode
  and corrupt-line tests, resume-ordering coverage, and session sidebar
  snapshots.

## alpha: Usable Coding Assistant

### Phase 11: Safe File Operations

- [x] Define write transcript event shape.
- [x] Implement create-file operation.
- [x] Implement exact-range replace operation.
- [x] Implement unified patch apply operation.
- [x] Record file path, operation type, and result for every write.
- [x] Preserve enough before/after metadata for session audit.
- [x] Unit-test create-file success and already-exists failure.
- [x] Unit-test exact-range replace success and stale-range failure.
- [x] Unit-test patch apply success and rejected patch failure.
- [x] Unit-test failed edits leave files unchanged.
- [x] Add snapshots for write success and write failure transcript entries.

### Phase 12: Shell Process Manager

- [ ] Define shell/process transcript event shapes.
- [ ] Run commands from the workspace by default.
- [ ] Show command, working directory, status, stdout, stderr, and elapsed time.
- [ ] Stream process output without blocking the TUI.
- [ ] Support command timeouts and cancellation.
- [ ] Keep a process registry for active commands.
- [ ] Track long-lived background processes separately from one-shot commands.
- [ ] Record command start, output summary, exit status, timeout, and
      cancellation as structured transcript/tool events.
- [ ] Prefer narrower built-in tools for file search, file reads, edits, and URL
      reads when they fit.
- [ ] Require approval for commands that write outside the workspace.
- [ ] Require approval for commands that access the network.
- [ ] Require approval for destructive commands.
- [ ] Detect and block shell-based attempts to bypass denied file or network
      operations where deterministic policy can identify them.
- [ ] Redact known secrets from displayed and recorded command output where
      deterministic redaction is possible.
- [ ] Unit-test process success, failure, timeout, and cancellation.
- [ ] Unit-test process registry handling for one-shot and background commands.
- [ ] Unit-test approval classification for destructive, network, and
      outside-workspace commands.
- [ ] Add snapshots for shell/process transcript entries.

### Phase 13: `read_url`

- [ ] Add a `read_url` tool with public `http`/`https` support.
- [ ] Reject private, loopback, link-local, local-file, and unsupported URL
      schemes.
- [ ] Enforce redirect, timeout, content-type, and response-size limits.
- [ ] Return final URL, status, title, readable text, truncation state, and
      diagnostics.
- [ ] Record `read_url` calls as structured transcript/tool events.
- [ ] Reuse existing Lectito-style extraction where it fits.
- [ ] Unit-test `read_url` public URL success and private URL rejection.
- [ ] Add snapshots for `read_url` transcript entries.

## v1: Supported Release

### Phase 14: Config, Inspect, and Export

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

### Phase 15: LSP and Code Intelligence

- [ ] Define read-only LSP tool names, inputs, outputs, and fallback behavior.
- [ ] Support document symbols.
- [ ] Support workspace symbols.
- [ ] Support go to definition.
- [ ] Support find references.
- [ ] Support hover.
- [ ] Support find implementations where the language server supports it.
- [ ] Degrade clearly when no language server is available.
- [ ] Keep LSP startup and indexing bounded with visible diagnostics.
- [ ] Record LSP calls as structured transcript/tool events.
- [ ] Preserve plain file search as the fallback path.
- [ ] Unit-test LSP fixture responses.
- [ ] Unit-test no-server fallback behavior.
- [ ] Add snapshots for LSP transcript entries.

### Phase 16: v1 Release Hardening

- [ ] Add `CHANGELOG.md` using Keep a Changelog categories.
- [ ] Document install flow.
- [ ] Document config file and env vars.
- [ ] Document session storage and inspect/export.
- [ ] Document Umans provider setup.
- [ ] Document search modes.
- [ ] Document `read_url`.
- [ ] Document shell/process manager behavior and approval rules.
- [ ] Document LSP/code-intelligence behavior and fallback rules.
- [ ] Document file-operation safety limits.
- [ ] Confirm packaging with `cargo package`.
- [ ] Confirm install path with local `cargo install --path .`.
- [ ] Add CI check plan for format, clippy, tests, and snapshots.
- [ ] Audit snapshots and session fixtures for secrets and machine-specific paths.
- [ ] Add ignored/manual Umans smoke-test instructions.
- [ ] Write release candidate checklist.
