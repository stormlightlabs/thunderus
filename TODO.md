# TODO

## Completed Summary

### Harness, Provider, And Event Loop

- Built the Clap entrypoint, initial terminal shell, prompt/transcript flow,
  deterministic fake agent stream, Umans provider client, Anthropic-compatible
  request lowering, streaming `AgentEvent` conversion, shared fake/Umans agent
  loop, cancellation, non-blocking UI updates, and gated live Umans smoke path.

### Context, Prompt Assembly, And Sessions

- Added root `AGENTS.md` loading, structured prompt bundles, thndrs-specific
  prompt fragments, provider-native tool schemas, model-visible transcript
  projection, `--print-prompt` redaction, append-only JSONL sessions, latest
  session resume, corrupt-line handling, and persisted context metadata.

### Tools And Safety Boundaries

- Added bounded read-only repository tools backed by `fd`, `rg --json`, and
  Rust-native file range reads; native/Exa/disabled web-search modes;
  Lectito-backed URL extraction; public-URL safety limits; private-network
  rejection; structured write/edit/patch operations with hashes and atomic
  failure behavior; and workspace-rooted shell/process execution with
  cancellation, timeout, truncation, audit metadata, and secret redaction.

### UI Experiments And Lessons

- Added Gridland-inspired transcript/status/prompt layout, banner states,
  syntax-highlighted transcript blocks, tool/error rendering, sidebar/session
  rendering, command/help surfaces, prompt dividers, file-mention suggestions,
  and broad Ratatui snapshots.
- Replaced the Ratatui-owned shell with a direct renderer after it proved too
  fragile for resize, cursor placement, prompt overlays, mouse scrolling,
  full-width rows, and native terminal scrollback.

### Prompt Input And Keybinds

- Added `PromptInput` with cursor-aware editing, multibyte-safe insert/delete,
  word motions, line start/end, newline insertion, submit/queue/history paths,
  file-path insertion, restore-on-failure, command-mode integration, inline
  cursor rendering, and unit coverage.
- Wired prompt-mode keybinds for arrows, `ctrl+b`, `ctrl+f`, `alt/ctrl` word
  motions, `home`, `end`, `ctrl+a`, `ctrl+e`, `shift+enter`, `ctrl+j`,
  `backspace`, and `delete`.

### Renderer Replacement

- Replaced the Ratatui-owned TUI shell with a direct renderer that uses crossterm
  for terminal I/O and a testable row model as the source of truth.

## Active Direction

The running TUI now uses a direct renderer:

- committed transcript blocks go to native terminal scrollback;
- only prompt, status, picker/help/suggestions, and active streaming rows are
  redrawn in a live region;
- rendering is driven by a testable row model;
- crossterm owns terminal I/O;
- Ratatui is no longer the source of truth for inline layout.

See:

- [Roadmap](ROADMAP.md)
- [Prompt Editing Libraries and Renderer Ownership](docs/internal/notes/prompt-renderer-research.md)
- [Text Input Library Lessons](docs/internal/notes/text-input-libraries.md)

## Backlog

### Refactor

- [ ] Move Anthropic-compatible stream accumulation out of `src/agent.rs` and
      into provider/protocol code that returns provider-neutral turn output.
- [ ] Move OpenAI-compatible chat stream accumulation out of `src/agent.rs` and
      into provider/protocol code that returns provider-neutral turn output.
- [ ] Keep retry, cancellation, tool-loop budgeting, steering, and tool
      execution orchestration in the agent loop.
- [ ] Add focused tests for provider stream normalization using existing SSE
      fixtures.
- [ ] Introduce a tool executor/registry shape where each tool module owns its
      definition, input parsing, execution, and output mapping.
- [ ] Generate the model-visible tool catalog from the tool registry.
- [ ] Replace the large tool dispatch match with registry lookup and executor
      calls.
- [ ] Add tests proving every registered tool has a stable schema and dispatch
      path.
- [ ] Introduce a small runtime/run controller for active agent slot,
      cancellation, steering sender, run spawning, event draining, and
      lifecycle logging.
- [ ] Keep terminal event polling/rendering in `lib.rs` and move agent
      lifecycle glue into the runtime/run controller.
- [ ] Define a stable semantic turn-event layer between `AgentEvent`,
      transcript `Entry`, and append-only session records.
- [ ] Project semantic turn events into transcript entries in one tested path.
- [ ] Project semantic turn events into session records in one tested path.
- [ ] Precompute renderer view geometry and row groups before terminal output.
- [ ] Add focused renderer snapshots for computed transcript, prompt,
      accessory, and status regions.
- [ ] Introduce input/app command enums so raw terminal keys are translated
      before mutating `App`.
- [ ] Move mode-specific key handling toward command translation tests.

### Config, Inspect, And Export

- [ ] Define config file path.
- [ ] Define config keys for model, web search mode, session path, tick/render
      rate, skill dirs, and default workspace behavior.
- [ ] Implement config loading.
- [ ] Implement precedence: CLI flags override env vars, env vars override
      config, config overrides built-in defaults.
  - [ ] Add non-TUI session inspect/export command.
  - [ ] Keep inspect/export output JSON or JSONL.
- [ ] Include loaded `AGENTS.md` files, scopes, hashes, and truncation state in
      inspect/export output.
- [ ] Include renderer-independent message metadata needed for later rendering.

### LSP And Code Intelligence

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

### Skill Engine And Self-Knowledge

- [x] Discover skills from configured skill dirs.
- [x] Validate skill metadata before exposing it to the model.
- [x] Load full skill instructions only after activation.
- [x] Bound skill reference traversal with depth, byte, file, and cycle limits.
- [x] Record activated skills and loaded references in session metadata.
- [x] Add a stable `thndrs` self-description fragment.
- [x] Expose a compact model-visible map of local docs and runtime state.
- [x] Add self-knowledge snapshots for prompt fragments, tools, skills,
      renderer mode, provider/model, search mode, and diagnostics.

### Prompt Input Correctness

- [x] Add `unicode-segmentation` for grapheme and Unicode word boundaries.
- [x] Keep `unicode-width` as the source of truth for terminal cell
      measurement.
- [x] Make left/right, backspace/delete, transpose, and cursor placement respect
      grapheme clusters.
- [x] Evaluate Unicode word boundaries for `alt/ctrl` word movement instead of
      whitespace-only scanning.
- [x] Add prompt tests for combining marks, emoji sequences, CJK, zero-width
      characters, wide characters, explicit newlines, and wrapped rows.

## Parking Lot

- [ ] Tool call failures should have debuggable logs and more information about
      why in the transcript.
- [ ] Keybinds should be readline-like
- [ ] Text should selectable in messages and input
- [ ] The app should be scrollable
- [ ] Git status should be in the statusline
- [ ] Model switcher for Umans
- [ ] OpenCode Go support
