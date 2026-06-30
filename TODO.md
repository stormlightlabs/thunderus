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

### Safe File Operations

- Added structured write transcript events, create-file, exact-range replace,
  and unified patch application, file path/operation/result audit metadata,
  before/after hashes and sizes, failure atomicity, unit coverage for success
  and stale/rejected edits, and snapshots for write success and failure states.

### Shell Process Manager

- Added workspace-rooted process execution, structured process transcript
  events, stdout/stderr/status/elapsed-time rendering, non-blocking output
  capture, timeouts, cancellation, one-shot/background process registry
  scaffolding, local-process security prompt/docs, secret redaction, truncation
  and full-output persistence coverage, process unit tests, and shell snapshots.

### `read_url`

- Added public `http`/`https` URL reading, private/loopback/link-local/local-file
  rejection, redirect/timeout/content-type/response-size limits, final URL,
  status, title, readable text, truncation, diagnostics, structured transcript
  events, Lectito-style extraction reuse, public/private fixture tests, and
  snapshots for success and failure states.

## alpha: Usable Coding Assistant

### Phase 14: Finalize Alpha

- [x] Complete the Umans tool-result feedback loop: append provider-native
      tool-result messages after each dispatched tool batch and re-request until
      the model stops requesting tools or the tool-iteration cap is hit.
- [x] Remove the redundant legacy `ToolInput`/`execute` path or make it the
      single dispatch path; provider dispatch, transcript rendering, and session
      records should share one obvious tool execution route.
- [x] Wire the existing command/help mode scaffolding into the TUI instead of
      leaving `Mode::Command` and `Mode::Help` dormant.
- [x] Wire existing stop/error run states into cancellation/failure rendering so
      `RunState::Stopping` and `RunState::Error` represent real app states.
- [x] Wire existing background-process registry controls into the live app:
      start, list, inspect, cancel, and clean up background commands through
      transcripted process events.
- [x] Remove or justify every remaining `#[allow(dead_code)]` after the above
      wiring, including provider/session module exports, shell registry helpers,
      theme constants, write-patch helpers, and search module-level suppression.
- [x] Resolve adjacent TODOs that mark broken-ground surfaces: provider request
      shape, Unicode status icons, `sse_to_agent_event` conversion cleanup,
      prompt snapshot intent, private-URL test table cleanup, and path dispatch.
- [x] Add `syntect`-backed syntax highlighting for code-oriented transcript
      blocks using cached syntax/theme sets, extension/language detection, and
      a small mapping from syntect colors/styles into Ratatui spans.
- [x] Highlight only code fences, file snippets, diffs, and command output that
      benefits from it; keep plain prose and status rows unhighlighted.
- [x] Fix run-state/status consistency in snapshots: running shell/tool states
      should show `running tool`, completed `read_url` should not leave the
      prompt at `sending`, and failed/timeout tool-only turns should surface as
      failed instead of sidebar `done`.
- [x] Add width-aware ellipsis truncation for user prompts, assistant/reasoning
      text, tool argument summaries, URLs, diagnostics, error messages, prompt
      input, and footer fields; current snapshots cut words and URLs without an
      ellipsis.
- [x] Standardize transcript leading columns: role rows, streaming assistant
      rows, reasoning rows, and tool rows should use one stable label/gutter
      grid so spinners and chips do not shift the message column.
- [x] Redesign tool output blocks so command summaries, stdout/stderr headers,
      nested output lines, and truncation markers share one aligned gutter and
      keep section headers visually distinct from output content.
- [x] Fix error row layout: error icons and wrapped/continued error text should
      align under the message body, not after a large role-label gap or clipped
      hard at the panel edge.
- [x] Tune narrow-width layout: hide the sidebar earlier or compress footer
      fields so `cwd`, long model names, prompts, and transcript rows truncate
      intentionally at 40-50 columns.
- [x] Tune empty-state/banner layout: center or optically align the banner at
      normal width, make the fallback empty state feel intentional at 50 columns,
      and avoid large uneven blank regions.
- [x] Add Gridland-style transcript group spacing: one-cell horizontal padding,
      one-cell gaps between semantic message groups, bottom padding near the
      prompt, and bottom-sticky scrolling that keeps newest content readable.
- [x] Evaluate role-specific message shells: keep assistant/tool rows left
      aligned, but render user prompts as a visually distinct bounded block or
      right/indented row instead of only a fixed-width `User` label.
- [ ] Keep reasoning as a sibling block with a stable header/status line
      (`Thinking`/`Thought`, running/done) and aligned body content, matching the
      Gridland source pattern without nesting it inside assistant text.
- [ ] Split prompt rendering into compound subregions: divider, suggestions,
      input row, status/error text, submit/stop indicator, and model/footer
      metadata, so each piece can be aligned and tested independently.
- [ ] Add slash-command and file-mention suggestion UI above the prompt for the
      already-started command/help surface; include selection marker, command
      description, history navigation, and dismissal behavior in snapshots.
- [ ] Make prompt dividers carry focus/run state: normal, submitted, streaming,
      and error states should have distinct solid/dashed divider styling or
      color, while preserving full-width alignment through the panel gutters.
- [ ] Add sidebar focus/selection semantics inspired by Gridland `SideNav`:
      separate active session from main-panel interaction, show shortcut hints
      such as `↑↓ navigate`, `enter select`, `esc back`, and reserve room for
      session suffixes/badges.
- [ ] Preserve prompt input on async/provider submit failure so the user can
      retry or edit, and add snapshots for failed submit with input retained.
- [ ] Add/refresh snapshots for syntax-highlighted code fences, diffs, Rust
      compiler errors, JSON/tool diagnostics, and plain prose to verify
      highlighting does not color gutters, borders, or status chips.
- [ ] Run and document the alpha readiness checks: `cargo fmt`, `cargo check`,
      clippy, unit tests, and snapshot review.
- [ ] Add an alpha smoke workflow that covers prompt assembly, a read-only tool,
      a file write, `read_url`, one shell success, one shell failure, session
      resume, and cancellation.

## v1: Supported Release

### Phase 15: Config, Inspect, and Export

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

### Phase 16: LSP and Code Intelligence

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

### Phase 17: v1 Release Hardening

- [ ] Add `CHANGELOG.md` using Keep a Changelog categories.
- [ ] Document install flow.
- [ ] Document config file and env vars.
- [ ] Document session storage and inspect/export.
- [ ] Document Umans provider setup.
- [ ] Document search modes.
- [ ] Document `read_url`.
- [ ] Document shell/process manager behavior and local-process security model.
- [ ] Document LSP/code-intelligence behavior and fallback rules.
- [ ] Document file-operation safety limits.
- [ ] Confirm packaging with `cargo package`.
- [ ] Confirm install path with local `cargo install --path .`.
- [ ] Add CI check plan for format, clippy, tests, and snapshots.
- [ ] Audit snapshots and session fixtures for secrets and machine-specific paths.
- [ ] Add ignored/manual Umans smoke-test instructions.
- [ ] Write release candidate checklist.
