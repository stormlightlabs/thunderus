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

- [v0 / Alpha Renderer Spec](docs/internal/specs/v0.md)
- [v1 Spec](docs/internal/specs/v1.md)
- [Prompt Editing Libraries and Renderer Ownership](docs/internal/notes/prompt-renderer-research.md)
- [Text Input Library Lessons](docs/internal/notes/text-input-libraries.md)

## v1 Backlog

### Config, Inspect, And Export

- [ ] Define config file path.
- [ ] Define config keys for model, web search mode, session path, tick/render
      rate, skill roots, and default workspace behavior.
- [ ] Implement config loading.
- [ ] Implement precedence: CLI flags override env vars, env vars override
      config, config overrides built-in defaults.
- [ ] Add non-TUI session inspect/export command.
- [ ] Keep inspect/export output JSON or JSONL.
- [ ] Include loaded `AGENTS.md` files, scopes, hashes, and truncation state in
      inspect/export output.
- [ ] Include renderer-independent message metadata needed for later
      re-rendering.
- [ ] Document session/config compatibility expectations.
- [ ] Document `AGENTS.md` precedence: harness policy, user prompt, CLI/config,
      nearest `AGENTS.md`, broader `AGENTS.md`, defaults.
- [ ] Document nested `AGENTS.md` scoping for v1 or mark it explicitly deferred.
- [ ] Unit-test config precedence.
- [ ] Integration-test `--help`.
- [ ] Integration-test inspect/export against fixture sessions.
- [ ] Integration-test inspect/export includes `AGENTS.md` context metadata.

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

- [ ] Discover skills from configured skill roots.
- [ ] Validate skill metadata before exposing it to the model.
- [ ] Load full skill instructions only after activation.
- [ ] Bound skill reference traversal with depth, byte, file, and cycle limits.
- [ ] Record activated skills and loaded references in session metadata.
- [ ] Add a stable `thndrs` self-description fragment.
- [ ] Expose a compact model-visible map of local docs and runtime state.
- [ ] Add self-knowledge snapshots for prompt fragments, tools, skills,
      renderer mode, provider/model, search mode, and diagnostics.

### Prompt Input Correctness

- [ ] Add `unicode-segmentation` for grapheme and Unicode word boundaries.
- [ ] Keep `unicode-width` as the source of truth for terminal cell
      measurement.
- [ ] Make left/right, backspace/delete, transpose, and cursor placement respect
      grapheme clusters.
- [ ] Evaluate Unicode word boundaries for `alt/ctrl` word movement instead of
      whitespace-only scanning.
- [ ] Add prompt tests for combining marks, emoji sequences, CJK, zero-width
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
