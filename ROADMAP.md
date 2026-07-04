# Roadmap

## References

- [Prompt Editing Libraries and Renderer Ownership](docs/internal/notes/prompt-renderer-research.md)
- [Text Input Library Lessons](docs/internal/notes/text-input-libraries.md)
- [Configuration Plan](docs/internal/features/003_configuration/plan.md)
- [Sessions Plan](docs/internal/features/005_sessions/plan.md)
- [Tool Registry Plan](docs/internal/features/007_tool_registry/plan.md)
- [MCP Plan](docs/internal/features/008_mcp/plan.md)
- [Pi Coding Agent Harness Lessons](docs/internal/notes/pi.md)
- [Ratatui Application Patterns](docs/internal/notes/ratatui.md)
- [Ratatui Snapshot Testing](docs/internal/notes/ratatui-testing.md)
- [UI Patterns](docs/internal/notes/ui-patterns.md)
- [Agent Skills Specification](docs/internal/notes/skills.md)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
- [Rust CLI Book: Testing](https://rust-cli.github.io/book/tutorial/testing.html)
- [Rust CLI Book: Packaging](https://rust-cli.github.io/book/tutorial/packaging.html)

## Architecture Refactors

- Normalize provider wire streams before they enter the app. Provider modules
  should own Anthropic/OpenAI/Umans/OpenCode protocol quirks and emit a small
  provider-neutral stream/turn shape. The agent loop should own retry,
  cancellation, tool orchestration, and turn budgeting, not SSE details.
- Turn tools into executable registry entries. Each tool module should own its
  model-visible definition, input parsing, execution, and output mapping. The
  catalog should be derived from the registry rather than manually synchronized
  with a large dispatch match.
- Introduce a small runtime/run controller between terminal I/O and app state.
  It should own the active agent slot, steering sender, cancellation, run
  spawning, event draining, and lifecycle logging. `App` should remain focused
  on state and message updates.
- Split runtime events, durable turn events, and renderer entries. Provider and
  tool events should reduce to stable semantic turn events before they become
  transcript rows or append-only session records.
- Precompute renderer view geometry and row groups before drawing. Rendering
  should read a pure view model for transcript blocks, prompt/accessory rows,
  status, and future hit areas instead of mixing layout decisions with terminal
  output.
- Move raw key handling toward command primitives. Terminal keys should map to
  small input/app commands, and those commands should drive `App` updates.

## Requirements

- Non-TUI session inspect/export commands.
- TUI and CLI session operations for listing, showing, resuming, inspecting,
  exporting, and reading bounded runtime logs.
- Explicit durable memory in the context-control plan, with inspectable
  Markdown source and rebuildable SQLite metadata, FTS5, and sqlite-vec indexes.
- Read-only LSP-backed code intelligence where a language server exists.
- MCP external tools only after built-in tools are registry-backed and
  namespaced external execution can reuse the same audit path.
- Release notes follow a changelog format.
- Packaging supports at least `cargo install`.
- CI runs formatting, clippy, unit tests, renderer snapshots, integration tests,
  and no-network provider fixture tests.
- Upgrade behavior is documented for session changes.

## Session Inspect And Export

Non-TUI commands should output JSON or JSONL and include:

- Transcript entries.
- Provider/model metadata.
- Tool events.
- Search and URL-read metadata.
- Loaded `AGENTS.md` paths, scopes, hashes, and truncation state.
- Activated skills and loaded skill reference metadata.
- File-operation audit metadata.
- Renderer-independent message metadata needed for later re-rendering.

Inspect/export must not leak API keys or machine-specific transient data.

## Search And File Discovery

Repository file discovery prefers `fd`; content search prefers `rg --json`.

Required fallback behavior:

- If `fd` is missing, use a bounded fallback that preserves workspace
  containment and ignores common generated/vendor directories.
- If `rg` is missing, use a bounded fallback that preserves output caps and
  marks the result as degraded.
- Fallback choice appears in diagnostics, transcript/tool metadata, and tests.

## LSP And Code Intelligence

Add read-only LSP-backed tools only when they are clearly better than plain file
search.

Supported operations:

- Document symbols.
- Workspace symbols.
- Go to definition.
- Find references.
- Hover.
- Find implementations where the server supports it.

Required behavior:

- Never edit files through LSP.
- Prefer existing installed language servers and degrade clearly when none is
  available.
- Keep startup and indexing bounded with visible diagnostics.
- Record LSP calls as structured transcript/tool events.
- Preserve plain file search as the fallback path.

## Additional Fits

These fit if they remain narrow and reuse the existing session and tool
machinery:

- Session title generation from transcript metadata.
- Transcript chunk summaries for long sessions.
- Session search once persistence exists.
- Review and security-review prompt fragments that operate on explicit diffs or
  session events.

## Thunderus Legacy Extraction

The original `stormlightlabs/thunderus` repo is a reference for product
affordances, not architecture. The old workspace split and Iced GUI are not
future work for this repository.

Useful pieces have been split into internal plans:

- session history, resume, debug logs, and inspect/export:
  `docs/internal/features/005_sessions/`;
- durable memory kinds, explicit store/recall, sqlite-vec semantic recall, and
  context selection:
  `docs/internal/features/001_context_control/`;
- tool schema/dispatch co-location:
  `docs/internal/features/007_tool_registry/`;
- MCP server configuration and namespaced external tools:
  `docs/internal/features/008_mcp/`.

## Release Bar

- No known data-loss bugs.
- No known terminal cleanup bugs.
- No known prompt cursor bugs for multiline/wrapped input, including common
  grapheme clusters, emoji sequences, CJK text, and zero-width marks.
- No known secret leakage in logs, snapshots, sessions, or errors.
- All non-network tests pass from a clean checkout.
- Manual Umans smoke test passes with `UMANS_API_KEY`.
- Packaging smoke test passes from a local package artifact.
- Known limitations are documented.

## Required Checks

- Renderer row-model tests and snapshots.
- Prompt input tests for grapheme-aware insert, delete, backspace, cursor
  movement, word movement, wrapping, and cursor placement.
- Long-prompt benchmark or spike covering `String` versus Ropey before adopting
  a rope-backed prompt buffer.
- Inspect/export integration tests.
- LSP fixture tests and no-server fallback tests.
- Skill metadata validation and progressive-loading tests.
- Remote skill fetch safety and reference-depth tests.
- Agent self-knowledge snapshot tests.
- Local `cargo package`.
- Local install-path smoke test.

## Release Artifacts

- `CHANGELOG.md` using Keep a Changelog categories.
- Install documentation.
- Session inspect/export documentation.
- Umans provider setup documentation.
- Search mode and fallback documentation.
- Renderer behavior documentation.
- LSP/code-intelligence documentation.
- Skill authoring and skill loading documentation.
- Agent self-knowledge/introspection documentation.
- Release candidate checklist.
