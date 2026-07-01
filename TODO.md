# TODO

## Completed Summary

### Harness, Provider, And Event Loop

- Built the Clap entrypoint, initial Ratatui shell, prompt/transcript flow,
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
- Switched toward Codex/Pi-style inline scrollback, but the Ratatui-owned shell
  still proved too fragile for resize, cursor placement, prompt overlays, mouse
  scrolling, full-width rows, and native terminal scrollback.

### Prompt Input And Keybinds

- Added `PromptInput` with cursor-aware editing, multibyte-safe insert/delete,
  word motions, line start/end, newline insertion, submit/queue/history paths,
  file-path insertion, restore-on-failure, command-mode integration, inline
  cursor rendering, and unit coverage.
- Wired prompt-mode keybinds for arrows, `ctrl+b`, `ctrl+f`, `alt/ctrl` word
  motions, `home`, `end`, `ctrl+a`, `ctrl+e`, `shift+enter`, `ctrl+j`,
  `backspace`, and `delete`.

## Active Direction

Replace the Ratatui-owned inline shell with a direct renderer:

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

## Renderer Replacement Milestones

### Milestone 1: Row Model And Terminal Backend

- [x] Add renderer-owned row primitives for spans, styles, rows, blocks, and
      frames.
- [x] Add width-aware wrapping, padding, truncation, and ellipsis helpers that
      do not depend on Ratatui widgets.
- [x] Add cursor-coordinate calculation for prompt rows, including explicit
      newlines, wrapped lines, prompt indent, and multibyte text.
- [x] Add a crossterm terminal backend for size reads, cursor hide/show,
      clearing live rows, moving the cursor, writing rows, and cleanup.
- [x] Add row-model tests for narrow, normal, and wide widths.
- [x] Add prompt cursor tests for single-line, wrapped, multiline, indented, and
      multibyte input.
- [x] Add renderer snapshots that assert styled rows without using Ratatui as
      the layout engine.

### Milestone 2: Native Scrollback And Live Region

- [x] Print completed transcript blocks into terminal scrollback instead of
      redrawing them in an app-owned transcript viewport.
- [x] Redraw only the live region: active streaming block, dynamic status,
      prompt input, suggestions/picker/help, and static status.
- [x] Keep session name plus dynamic/reactive status above the input.
- [x] Keep static model/search/token/cwd status below the input with internal
      padding and width-aware clipping.
- [x] Leave wheel and trackpad scrolling to the terminal outside focused picker
      navigation.
- [x] Recompute live rows and cursor placement on resize.
- [x] Add smoke coverage for scrollback-friendly transcript output.

### Milestone 3: Message Blocks

- [ ] Render startup/banner content, user prompts, assistant text, reasoning,
      tools, process output, and errors through the same row/block pipeline.
- [ ] Make colored/background blocks full width with horizontal padding.
- [ ] Add vertical padding inside visually grouped blocks where it improves
      readability.
- [ ] Render labels above message bodies with role-specific colors.
- [ ] Keep user, assistant, reasoning, tool, process, and error body text
      aligned after wrapping.
- [ ] Port syntax highlighting to renderer spans for code fences, diffs,
      snippets, diagnostics, and useful command output.
- [ ] Add snapshots for startup, narrow startup, user/assistant/reasoning,
      tool output, errors, diffs, Rust compiler output, JSON, and plain prose.

### Milestone 4: Prompt, Picker, Help, And Mentions

- [ ] Render slash-command suggestions as live-region rows, not overlays.
- [ ] Render help as live-region rows, not an overlay.
- [ ] Render the file picker as live-region rows, not an overlay.
- [ ] Add `@` file mentions in prompt input.
- [ ] Keep `ctrl+p` as a direct file picker shortcut.
- [ ] Render accepted file mentions in the prompt with distinct styling from
      plain text, without changing provider-visible prompt semantics at first.
- [ ] Ensure `escape` closes picker/help/suggestions before it stops work or
      exits a broader mode.
- [ ] Match Codex-style picker states: empty `@` hint, loading state,
      stale-result guard if search becomes async, and clear `no matches` row.
- [ ] Match Codex-style selection rows: stable scroll window, selected marker,
      highlighted fuzzy match indices, and clipped long paths.
- [ ] Match Pi-style help organization: generic selection keys (`up`, `down`,
      `pageUp`, `pageDown`, `enter`, `escape`) appear in help/footer hints.
- [ ] Add file picker snapshots for empty query, filtered results, no matches,
      long path clipping, accepted mention styling, and scrolled selection.

### Milestone 5: Ratatui Migration Cleanup

- [ ] Route the running TUI through the direct renderer.
- [ ] Remove Ratatui from inline rendering paths.
- [ ] Keep or delete old Ratatui snapshots deliberately; do not leave duplicate
      snapshots for dead surfaces.
- [ ] Replace Ratatui `TestBackend` UI snapshots with row-model and terminal
      transcript snapshots.
- [ ] Audit terminal cleanup on normal exit, error exit, interrupt, panic path,
      and provider/tool cancellation.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo test`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.

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

## Parking Lot

- [ ] Tool call failures should have debuggable logs and more information about
      why in the transcript.
- [ ] Consider whether the direct renderer should eventually expose a public
      fixture format for external visual regression tests.

### Keybinds

#### Cursor

| Key        | Desc                       |
| ---------- | -------------------------- |
| ctrl+]     | Jump forward to character  |
| ctrl+alt+] | Jump backward to character |

#### Global

| Key            | Desc                                 |
| -------------- | ------------------------------------ |
| ctrl+d, ctrl+d | Quit after double-press confirmation |

#### File Picker

| Key    | Desc                                                 |
| ------ | ---------------------------------------------------- |
| @      | Start file mention picker from prompt input          |
| ctrl+p | Open file picker directly                            |
| escape | Close picker/help/suggestions before broader actions |
