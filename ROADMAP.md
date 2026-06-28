# ROADMAP

`thndrs` is a minimal Rust + Ratatui coding harness. The current scope is right
for a fake/v0 release and the first half of alpha, but it is too limited for a
release candidate or v1. A v1 needs stable CLI/config/session contracts, safe
file-edit behavior, packaging, release notes, and recovery paths in addition to
the UI and provider work.

## References

- [Ratatui: The Elm Architecture](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/)
- [Ratatui: Component Architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/)
- [Ratatui: Terminal and EventHandler](https://ratatui.rs/recipes/apps/terminal-and-event-handler/)
- [Ratatui: Handle CLI arguments](https://ratatui.rs/recipes/apps/cli-arguments/)
- [Ratatui: Testing with insta snapshots](https://ratatui.rs/recipes/testing/snapshots/)
- [Gridland AI Chat Interface](https://www.gridland.io/docs/blocks/ai-chat-interface)
- [Gridland Cells and Layout](https://www.gridland.io/docs/core-concepts/cells-and-layout)
- [Pi coding-agent article](https://mariozechner.at/posts/2025-11-30-pi-coding-agent/)
- [Pi repository](https://github.com/earendil-works/pi)
- [Herdr docs](https://herdr.dev/docs/)
- [Herdr repository](https://github.com/ogulcancelik/herdr)
- [Umans code docs](https://app.umans.ai/offers/code/docs#api-reference)
- [Umans model metadata](https://api.code.umans.ai/v1/models/info)
- [Lectito docs](https://lectito.stormlightlabs.org/docs/)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
- [Rust CLI Book: Testing](https://rust-cli.github.io/book/tutorial/testing.html)
- [Rust CLI Book: Packaging](https://rust-cli.github.io/book/tutorial/packaging.html)
- [Rust CLI Book: Config files](https://rust-cli.github.io/book/in-depth/config-files.html)

## Design Boundaries

### Cross-Release Scope

- A full-screen Ratatui app with a chat/workbench layout.
- A Clap-based CLI entrypoint that launches the TUI by default.
- A single-line prompt first, with multi-line editing deferred until it is clearly needed.
- A typed transcript that can show user messages, assistant text, tool events,
  status rows, and errors.
- A fake deterministic agent stream before real provider work.
- A first real provider: Umans Code.
- First-class support for `umans-coder` and `umans-glm-5.2`.
- Umans native server-side web search, with Lectito-backed local search and
  extraction as the fallback/verification path.
- Pure state/update tests.
- Pure layout/render tests using Ratatui's test backend.

### Out of Scope Through v1

- MCP.
- Sub-agents.
- Built-in plan mode or todo manager.
- Multi-provider abstraction before a second provider exists.
- Background process manager.
- Server/client detach and reattach.
- PTY panes or terminal multiplexing.
- Plugins, socket API, or external control protocol.
- Multi-line editor with full shell/editor keybindings.

### Deferred Past fake/v0

- Real provider calls.
- Session persistence.
- Local search/extraction.
- Read/write/edit tools.
- Config files.
- Packaging and release automation.

## Release Categories

### fake/v0: Harness Proof

Goal: prove the Ratatui shell, input loop, transcript rendering, event model,
fake stream, tool-event display, and test strategy.

Required:

- `thndrs` launches a full-screen TUI by default.
- Clap parses the first flat CLI shape.
- User can type, submit, clear, and quit.
- Fake agent stream emits assistant, reasoning, tool, done, and error states.
- Layout survives normal and narrow terminal sizes.
- Snapshot and unit tests cover the stable fake states.

Not required:

- Network calls.
- Real model/provider.
- File writes.
- Session persistence.
- Config files.
- Packaging beyond `cargo run`.

### alpha: Usable Coding Assistant

Goal: become useful on a real repository while the CLI/config/session contracts
are still allowed to change.

Required:

- Umans provider works with `umans-coder` and `umans-glm-5.2`.
- `UMANS_API_KEY` is the only secret path.
- Native Umans web search can be enabled/disabled.
- Root `AGENTS.md` and explicit context sources are visible in the transcript.
- Sessions persist as append-only JSONL and resume.
- Read-only local tools exist for file listing, file reading, and search.
- Safe file-edit tools exist behind explicit, narrow operations: create file,
  replace exact range, and apply unified patch.
- Stop/cancel and provider errors recover to a usable prompt.
- Local Lectito search/extraction is available as a fallback or inspection path.

Allowed instability:

- Session JSONL fields can change.
- CLI flags can still be renamed.
- Config file shape is optional or unstable.
- Live provider smoke tests can remain manual/ignored.

### v1: Supported Release

Goal: define the supported user-facing contract and make the harness safe enough
to install and use repeatedly.

Required:

- Stable CLI flags, environment variables, config file keys, and session format.
- `--help` documents the default model, search modes, config path, and session path.
- Config file support with CLI/env overrides and clear precedence.
- File write/edit behavior is documented, bounded, and recoverable.
- Transcript/session export or inspect command can read sessions without opening
  the TUI.
- Release notes follow a changelog format.
- Packaging supports at least `cargo install`; binary artifacts can follow.
- CI runs formatting, clippy, unit tests, snapshot tests, and no-network provider
  fixture tests.
- Upgrade behavior is documented for alpha session/config changes.

Release candidate bar:

- No known data-loss bugs.
- No known terminal cleanup bugs.
- No known secret leakage in logs, snapshots, sessions, or errors.
- All non-network tests pass from a clean checkout.
- Manual Umans smoke test passes with `UMANS_API_KEY`.

## CLI Entrypoint

Use `clap` derive syntax and keep the entrypoint flat. The binary should launch
the TUI when run with no subcommand.

```rust
#[derive(clap::Parser, Debug)]
#[command(version, about = "Minimal Rust + Ratatui coding harness")]
pub struct Cli {
    #[arg(long, default_value = ".")]
    pub cwd: std::path::PathBuf,

    #[arg(long, default_value = "umans-coder")]
    pub model: String,

    #[arg(long, value_enum, default_value_t = WebSearchMode::Native)]
    pub websearch: WebSearchMode,

    #[arg(long, default_value_t = 100)]
    pub tick_rate_ms: u64,

    #[arg(long)]
    pub no_alt_screen: bool,
}
```

```rust
#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebSearchMode {
    Native,
    Exa,
    None,
}
```

Rules:

- Do not add subcommands before alpha. v1 may add non-TUI `inspect`/`export`
  commands once sessions exist.
- Do not add a `--provider` flag while Umans is the only provider.
- `--cwd` controls context loading and display, not process-global `chdir`
  unless a later feature requires it.
- `--model` accepts `umans-coder` and `umans-glm-5.2` first.
- `--websearch` maps directly to `X-Umans-Websearch-Provider`.
- `--no-alt-screen` is for debugging and terminal-capture tests.
- Keep secrets out of CLI flags. Use `UMANS_API_KEY`.

## UI Contract

The first screen is the product. No splash screen.

```text
+----------------------+-----------------------------------------------+
| thndrs               | Transcript                                    |
|                      |                                               |
| Sessions             |  user     explain this repo                   |
| > scratch            |  assistant streaming response...              |
|                      |  tool     read Cargo.toml                     |
| Status               |                                               |
| idle                 |                                               |
|                      |                                               |
|                      | --------------------------------------------- |
|                      | > prompt text                                 |
|                      | model: fake-agent        cwd: /repo           |
+----------------------+-----------------------------------------------+
```

Layout rules:

- Sidebar width: fixed 22 columns for v0.
- Main area: remaining width.
- Footer: one line.
- Prompt: two lines when model/status label is visible; otherwise one line.
- Transcript: fills remaining height and shows newest entries.
- Minimum width handling: if terminal width is too small, hide sidebar before wrapping
  critical prompt/status text.

Borrowed patterns:

- From Gridland: sidebar + scrollable transcript + bottom prompt.
- From Herdr: compute view geometry before drawing, then render from stored rectangles.
- From Ratatui: Elm-style state/update/view loop.
- From Pi: event-streamed harness model and explicit context.

## State Model

Start with one `App` struct and plain enums. Do not add traits until there is more than
one implementation.

```rust
pub struct App {
    pub mode: Mode,
    pub run_state: RunState,
    pub input: String,
    pub transcript: Vec<Entry>,
    pub sidebar: Sidebar,
    pub view: ViewState,
}
```

```rust
pub enum Mode {
    Prompt,
    Command,
    Help,
}

pub enum RunState {
    Idle,
    Working,
    Stopping,
    Error(String),
}
```

```rust
pub enum Entry {
    User { text: String },
    Assistant { text: String, streaming: bool },
    Reasoning { text: String, streaming: bool },
    Tool { name: String, status: ToolStatus, output: Vec<String> },
    Status { text: String },
    Error { text: String },
}
```

```rust
pub enum Msg {
    Key(crossterm::event::KeyEvent),
    Tick,
    Submit,
    Clear,
    Quit,
    Agent(AgentEvent),
}
```

`update(&mut App, Msg) -> Option<Msg>` is the only mutation path. If a message should
cause follow-up work, return another `Msg` instead of reaching sideways from the event
loop.

## Event Model

Use a fake agent first, but shape it like a real runtime:

```rust
pub enum AgentEvent {
    Started,
    AssistantDelta(String),
    ReasoningDelta(String),
    ToolStarted { name: String },
    ToolOutput { line: String },
    ToolFinished,
    Finished,
    Failed(String),
}
```

Rules:

- User submit appends `Entry::User`.
- Agent start sets `RunState::Working`.
- Assistant deltas append to the latest streaming assistant entry, or create one.
- Reasoning deltas append to the latest streaming reasoning entry, or create
  one. Keep reasoning separate from final assistant text.
- Tool events create/update one tool entry.
- Finished marks streaming assistant/reasoning entries complete and returns to
  idle.
- Failed adds `Entry::Error` and returns to error/idle.

## Testing Strategy

Use the cheapest deterministic test at each layer:

- CLI parsing: unit tests around `Cli::try_parse_from`, including defaults,
  `--model`, `--websearch`, `--cwd`, and invalid search modes.
- State updates: pure unit tests for `update(&mut App, Msg)`.
- Layout geometry: pure tests for `compute_view(Rect)`.
- Rendering: Ratatui `TestBackend` plus `insta` snapshots at fixed terminal
  sizes. Start with `80x24` and one narrow size where the sidebar is hidden.
- Agent streams: deterministic fake stream tests before any real provider.
- Provider requests: no-network tests for request construction and stream
  fixture parsing.
- Live provider smoke tests: ignored/manual tests only, because they need
  `UMANS_API_KEY` and network.
- Search/extraction: fixture tests for DuckDuckGo parsing and Lectito extraction
  safety behavior before live web reads.

Snapshot rules:

- Snapshot whole screens only for stable UI states.
- Prefer small state fixtures: empty shell, submitted prompt, streaming answer,
  tool event, reasoning event, provider error, narrow layout.
- Never snapshot timestamps, session IDs, or machine-specific paths without
  redaction or fixed fixtures.
- Intentional UI changes are reviewed with `cargo insta review`.

## Provider Plan

The first provider is Umans Code. The docs expose both Anthropic-compatible and
OpenAI-compatible routes, but the initial harness should use the Anthropic
Messages API because GLM 5.2 vision is documented as `/v1/messages` only and
reasoning streams arrive as explicit `thinking` blocks.

Provider contract:

- Base URL: `https://api.code.umans.ai`.
- Primary endpoint: `POST /v1/messages`.
- Auth header: `x-api-key: $UMANS_API_KEY`.
- Required version header: `anthropic-version: 2023-06-01`.
- Model metadata endpoint: `GET /v1/models/info`.
- Default model: `umans-coder`.
- Alternate model: `umans-glm-5.2`.
- Optional OpenAI route later: `POST /v1/chat/completions` with bearer auth.

Model mapping:

- `umans-coder`: default coding model. Current metadata says it routes to
  Kimi K2.7-Code, supports tools and vision, has a 262,144-token context
  window, recommends 32,768 output tokens, and keeps reasoning enabled.
- `umans-glm-5.2`: explicit deep/context model. Current metadata reports a
  405,504-token context window, 131,071 recommended output tokens, tool
  support, and reasoning levels `none`, `high`, and `max` with `high` as the
  default.

Do not introduce a `Provider` trait in the first implementation. Start with a
concrete `UmansClient` and split out an interface only when a second provider
lands.

## Search and Extraction

Search has two lanes:

- Primary: Umans server-side web search. When a request carries a web-search
  tool, set `X-Umans-Websearch-Provider: native` for the Kimi-backed path.
  Keep `exa` and `none` as explicit config values, but default to `native`.
- Local fallback: reuse the local Lectito project at
  `~/Projects/StormlightLabs/OpenSource/lectito` for deterministic article
  extraction and DuckDuckGo HTML search prior art.

Lectito reuse should stay simple:

- Use the `lectito` crate for HTML-to-readable-content extraction when the
  harness has already fetched a page.
- Reuse the `lectito-mcp` DuckDuckGo HTML parser/search approach for local
  fallback search, including small result limits and bot-challenge detection.
- Reuse the existing `read_article` safety shape: public `http`/`https` URLs
  only, redirect limit, max fetch bytes, HTML content-type check, private
  network rejection by default, chunked output.

The TUI should render provider-native search as tool events even when Umans
executes the search server-side. Local Lectito search/extraction should use the
same transcript event shape so the UI does not care which lane produced the
result.

## File Plan

Keep the module tree small:

```text
src/cli.rs        Clap args, value enums, parse tests
src/main.rs       terminal setup and app run loop
src/app.rs        App, enums, update logic, state tests
src/ui.rs         ViewState, layout computation, render functions, render tests
src/agent.rs      fake agent stream, later Umans event adapter
src/umans.rs      concrete Umans API client once Phase 6 starts
src/session.rs    append-only JSONL sessions once alpha starts
src/tools.rs      read-only tools first, safe edits later
src/config.rs     v1 config loading and precedence
```

Do not add `components/`, `store/`, `dispatcher/`, `plugin/`, or `runtime/` directories
in the first pass.

## Implementation Phases

### Phase 1: CLI and Ratatui Shell

Release target: fake/v0.

Details:

- Add `ratatui`, `crossterm`, `clap`, and dev-only `insta`.
- Add `src/cli.rs` with the flat Clap entrypoint.
- Enter alternate screen and raw mode.
- Restore terminal on normal exit and panic.
- Poll key events.
- Draw static sidebar, transcript placeholder, prompt, and footer.

Acceptance:

- `cargo run` opens the workbench.
- `cargo run -- --help` prints Clap help.
- `q` and `Ctrl+C` exit cleanly.
- `cargo check` passes.

Testing:

- Unit-test CLI defaults and invalid `--websearch` values.
- Add the first `TestBackend` snapshot for the empty shell at `80x24`.

### Phase 2: Prompt and Transcript

Release target: fake/v0.

Details:

- Implement single-line input editing: printable chars, Backspace, Enter, Esc.
- Submit appends a user entry.
- `/clear` clears transcript.
- `/quit` exits.
- Transcript render shows newest entries fitting the viewport.

Acceptance:

- Update logic is unit-tested without terminal setup.
- Render test proves submitted text appears in the transcript.

Testing:

- Unit-test submit, clear, quit, printable input, and Backspace behavior.
- Snapshot the transcript after one submitted prompt.

### Phase 3: Fake Agent Stream

Release target: fake/v0.

Details:

- Add `agent.rs` fake stream with timed events.
- Wire stream events into the app loop through a channel.
- Render streaming assistant text.
- Render one fake tool call block.
- Add stop handling if the fake stream is active.

Acceptance:

- Submitting a prompt produces a deterministic streaming response.
- Tool start/output/end appear as separate transcript rows.
- Stop returns app to idle without leaving a streaming entry stuck.

Testing:

- Unit-test every `AgentEvent` transition, including reasoning deltas.
- Snapshot streaming assistant, reasoning, tool-running, and finished states.

### Phase 4: Layout Hardening

Release target: fake/v0.

Details:

- Add `ViewState` with rects for sidebar, transcript, prompt, and footer.
- Compute geometry before rendering.
- Hide sidebar under a small-width threshold.
- Add Ratatui `TestBackend` snapshot-style assertions for key regions.

Acceptance:

- UI does not panic or overlap at narrow and normal terminal sizes.
- Layout tests cover desktop and narrow modes.

Testing:

- Unit-test `compute_view` with normal, narrow, and tiny terminal rects.
- Snapshot normal and narrow layouts.

### Phase 5: Context and Read-Only Tool Boundary

Release target: alpha.

Details:

- Define a `ToolEvent` or reuse `AgentEvent::Tool*`.
- Load explicit context: prompt, transcript tail, root `AGENTS.md` if present.
- Show loaded context sources in a transcript/status entry.
- Add read-only local helpers: `list_files`, `read_file`, and `grep`.
- Keep tool output structured for UI rendering.
- Do not implement write/edit tools in this phase.

Acceptance:

- Tool entries render without parsing assistant text.
- Tool functions are testable as normal Rust functions.
- Context sources are visible before the model response starts.

Testing:

- Unit-test structured tool entry updates.
- Snapshot successful and failed tool entries.

### Phase 6: One Real Model Path

Release target: alpha.

Details:

- Implement Umans Code as the first provider.
- Read `UMANS_API_KEY` from the environment.
- Use `umans-coder` by default.
- Allow switching to `umans-glm-5.2`.
- Fetch `/v1/models/info` for visible model/capability metadata.
- Send Anthropic-compatible streaming requests to `/v1/messages`.
- Map text deltas, thinking deltas, tool events, completion, and errors into
  `AgentEvent`.
- Enable Umans native web search with `X-Umans-Websearch-Provider: native`
  when the model receives a web-search tool.
- Stream provider text into `AgentEvent`.
- Load explicit context: prompt, transcript tail, root `AGENTS.md` if present.
- Show context sources in a status entry.

Acceptance:

- Umans can answer in the TUI with `umans-coder`.
- `umans-glm-5.2` can be selected without changing code.
- Native web search can be enabled and its activity appears in the transcript.
- No hidden context is injected.
- Provider errors become transcript errors.

Testing:

- Unit-test request construction without network.
- Unit-test stream parsing from checked-in fixtures.
- Unit-test model metadata parsing for `umans-coder` and `umans-glm-5.2`.
- Keep live Umans smoke tests ignored/manual and gated on `UMANS_API_KEY`.

### Phase 7: Search and Extraction

Release target: alpha.

Details:

- Keep Umans native search as the default path.
- Add a local search/extraction helper only after Umans native search works.
- Reuse Lectito's existing extraction API and DuckDuckGo HTML search prior art.
- Keep local search read-only and bounded.
- Render Lectito search/extraction through the same `AgentEvent::Tool*` path.

Acceptance:

- A query can produce bounded local search results when Umans server-side search
  is disabled or unavailable.
- A selected public URL can be extracted to Markdown/text with truncation
  metadata.
- Private-network URLs and oversized documents fail closed.

Testing:

- Unit-test DuckDuckGo HTML parsing with fixtures.
- Unit-test bot-challenge detection.
- Unit-test URL safety and response-size guards.
- Unit-test Lectito extraction with local HTML fixtures.

### Phase 8: Session Persistence

Release target: alpha.

Details:

- Save transcript as JSONL.
- Resume latest session.
- Show session list in sidebar.
- Keep branching/forking out until resume is reliable.

Acceptance:

- Restart restores the last transcript.
- Session files are readable and append-only.

Testing:

- Unit-test JSONL encode/decode round trips.
- Unit-test resume ordering and corrupt-line handling.
- Snapshot sidebar session-list rendering.

### Phase 9: Safe File Operations

Release target: alpha for guarded file edits; v1 for stable behavior.

Details:

- Add narrowly scoped write operations only after read-only tools and sessions work.
- Support create-file, exact-range replace, and unified patch apply.
- Record every write operation as a structured transcript/tool event.
- Store enough before/after metadata in the session to audit what changed.
- Keep long-running shell/process execution out of scope.

Acceptance:

- A model/tool path can propose and apply a small file edit.
- Failed edits leave the file unchanged.
- The transcript shows the file path, operation type, and result.
- User can recover by inspecting session events and normal git diff output.

Testing:

- Unit-test create-file, exact-range replace, and patch apply success/failure.
- Unit-test that failed edits do not partially write.
- Fixture-test transcript entries for write success and write failure.

### Phase 10: Config, Inspect, and Export

Release target: v1.

Details:

- Add a config file with documented path and keys.
- Define precedence: CLI flags override env vars; env vars override config;
  config overrides built-in defaults.
- Add non-TUI inspect/export commands only when sessions exist.
- Keep machine-readable output JSON or JSONL.
- Document session/config compatibility expectations.

Acceptance:

- `thndrs --help` explains config, env vars, model, search, and session paths.
- A user can configure default model/search mode without passing flags every run.
- A user can inspect or export a session without launching the TUI.

Testing:

- Unit-test config precedence.
- Integration-test `--help`.
- Integration-test inspect/export against fixture sessions.

### Phase 11: v1 Release Hardening

Release target: v1 release candidate and v1.

Details:

- Add `CHANGELOG.md` using Keep a Changelog categories.
- Document install, config, sessions, provider setup, search modes, and safety limits.
- Confirm packaging through `cargo install`.
- Define release candidate checklist.
- Keep all network tests ignored/manual.

Acceptance:

- Clean checkout passes formatting, clippy, unit tests, snapshot tests, and
  no-network fixture tests.
- Manual Umans smoke test passes with `UMANS_API_KEY`.
- Terminal cleanup works on normal exit, error exit, and panic path.
- Sessions/config do not leak secrets.
- v1 known limitations are documented.

Testing:

- CI runs all non-network checks.
- Smoke-test packaging from a local package artifact.
- Audit snapshots and session fixtures for machine-specific paths/secrets.

## Herdr Lessons to Keep Small

Useful now:

- Explicit modes.
- Semantic run/agent states.
- Precompute view geometry.
- Test layout with plain `Rect` expectations.

Useful later:

- Prefix key model if `thndrs` embeds terminals.
- BSP pane layout if `thndrs` grows split panes.
- CLI/socket control if external orchestration becomes necessary.

Not useful for v0:

- Server/client runtime.
- PTY persistence.
- Live handoff.
- Plugin marketplace.
- Worktree orchestration.
