# Roadmap

## References

- [Prompt Editing Libraries and Renderer Ownership](docs/internal/notes/prompt-renderer-research.md)
- [Text Input Library Lessons](docs/internal/notes/text-input-libraries.md)
- [Pi Coding Agent Harness Lessons](docs/internal/notes/pi.md)
- [Ratatui Application Patterns](docs/internal/notes/ratatui.md)
- [Ratatui Snapshot Testing](docs/internal/notes/ratatui-testing.md)
- [UI Patterns](docs/internal/notes/ui-patterns.md)
- [Agent Skills Specification](docs/internal/notes/skills.md)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
- [Rust CLI Book: Testing](https://rust-cli.github.io/book/tutorial/testing.html)
- [Rust CLI Book: Packaging](https://rust-cli.github.io/book/tutorial/packaging.html)
- [Rust CLI Book: Config files](https://rust-cli.github.io/book/in-depth/config-files.html)

## Completed Renderer Foundation

The direct renderer work established the terminal interface direction:

- Completed transcript blocks are committed into native terminal scrollback.
- A small live region redraws prompt, status, picker/help, suggestions, and
  active streaming content.
- Mouse wheel and trackpad scrolling are left to the terminal except where a
  focused picker explicitly handles selection.
- Prompt input supports multiline editing, wrapping, cursor movement, and
  history without cursor drift.
- Resize recomputes rows and cursor position deterministically.
- Rendering is testable without Ratatui widgets as the source of truth.

The direct renderer owns:

- terminal size and event integration;
- row layout, wrapping, padding, truncation, and ANSI style output;
- cursor placement;
- committed transcript writes;
- live-region clear/redraw;
- resize invalidation.

Ratatui may remain useful for tests, references, or alternate-screen
experiments, but it is not the source of truth for inline rendering.

## Renderer Contract

The supported terminal UI should guarantee:

- Completed transcript blocks remain in native terminal scrollback.
- The live region redraws only prompt, status, active streaming output, picker,
  help, and suggestions.
- Wheel and trackpad scrolling are not captured for transcript history.
- Resize reflows prompt/accessory rows and keeps cursor placement correct.
- Startup, prompt, status, transcript, tool output, file picker, help, and
  narrow-width states have stable snapshot coverage.
- Missing `fd`/`rg` degrades with visible diagnostics and bounded fallbacks.

Prompt editing should use Unicode-aware text boundaries:

- Use `unicode-width` for terminal cell measurement.
- Use `unicode-segmentation` for grapheme and word boundaries where cursor
  movement, deletion, selection, and word motion need user-visible text units.
- Keep provider-visible prompt text as plain UTF-8 text; styling and accepted
  file mentions must not change submitted semantics.
- Benchmark long-prompt editing before replacing the prompt backing store with
  a rope or other editor buffer.

## Message Rendering

Message blocks should follow the Gridland/Codex/Pi direction:

- full-width backgrounds for colored blocks;
- one-cell horizontal padding inside blocks;
- vertical padding inside blocks where the block is visually grouped;
- labels above the body, not cramped into the body text;
- role-specific label colors;
- assistant, reasoning, tool, process, error, and user blocks share one wrapping
  and padding path;
- long paths, commands, URLs, and diagnostics truncate or wrap intentionally;
- syntax highlighting applies only to code fences, diffs, snippets, and useful
  command output.

## Prompt And Accessories

The prompt remains an internal model rather than a prompt-library-owned editor.

Expected behavior:

- multiline insertion with `shift+enter` and `ctrl+j`;
- cursor-aware editing across wrapped and explicit newline rows;
- history navigation that preserves the current draft when appropriate;
- stable prompt indent on every visual row;
- slash-command suggestions rendered as rows, not overlays;
- file picker and `@` mentions rendered as rows, not overlays;
- `escape` closes picker/help/suggestions before it stops work or exits a mode;
- accepted file mentions can be styled distinctly in the prompt without changing
  provider-visible prompt semantics at first.

## Public Contract

The supported user-facing contract includes:

- CLI flags, subcommands, and exit codes.
- Config file path, keys, and precedence.
- Environment variables.
- Session/event storage format and compatibility policy.
- Non-TUI inspect/export output shapes.
- Tool names, inputs, outputs, errors, and audit metadata.
- Renderer behavior that users can rely on.
- Search fallback behavior when `fd` or `rg` is missing.
- LSP tool names, inputs, outputs, and no-server fallback behavior.
- Skill discovery, activation, loading, validation, and precedence rules.
- Agent self-description/introspection output shape.
- Documented install and upgrade workflow.

Implementation modules and renderer internals are not public API. The row model
can change as long as the visible behavior and documented CLI/session contracts
hold.

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

- Direct inline renderer is the default terminal UI.
- Renderer behavior is documented for native scrollback, live prompt/status
  redraw, resize, mouse scrolling, multiline input, file picker, help, and
  `@` mentions.
- Config file support with CLI/env overrides and clear precedence.
- Non-TUI session inspect/export commands.
- Read-only LSP-backed code intelligence where a language server exists.
- Skill engine support for reusable task instructions with progressive
  disclosure.
- Agent self-knowledge for current prompt assembly, loaded resources, active
  tools, and `thndrs` docs.
- Release notes follow a changelog format.
- Packaging supports at least `cargo install`.
- CI runs formatting, clippy, unit tests, renderer snapshots, integration tests,
  and no-network provider fixture tests.
- Upgrade behavior is documented for session/config changes.

## CLI And Config

Add non-TUI `inspect` and `export` commands for persisted sessions.

Config precedence:

1. CLI flags.
2. Environment variables.
3. Config file.
4. Built-in defaults.

Config should cover:

- Default model.
- Web search mode.
- Session path.
- Tick/render rate if still configurable.
- Default workspace behavior.
- Skill roots.
- Optional LSP enablement.

Secrets stay out of config examples. `UMANS_API_KEY` remains the secret path.

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

Out of scope:

- Code actions.
- Automatic refactors.
- Untracked LSP server processes that outlive the session.
- Project-wide task management.

## Skill Engine

Add a small skill engine modeled on the Agent Skills specification, without
importing a plugin marketplace, installer, or multi-agent framework.

Supported skill shape:

- A skill is a directory containing `SKILL.md`.
- `SKILL.md` must contain YAML frontmatter followed by Markdown instructions.
- Required frontmatter: `name` and `description`.
- Optional frontmatter: `license`, `compatibility`, `metadata`, and
  `allowed-tools`.
- Optional directories: `scripts/`, `references/`, and `assets/`.

Required behavior:

- Discover skills from configured skill roots.
- Validate skill metadata before exposing it to the model.
- Load only skill metadata at startup.
- Load the full `SKILL.md` only after the model or harness activates the skill
  for the current task.
- Load referenced files only on demand, using paths relative to the skill root.
- Bound deep reference traversal with a max depth, total byte budget, per-file
  byte cap, cycle detection, and visible diagnostics when traversal stops.
- Record activated skill names, versions/hashes, and loaded reference paths in
  session metadata.
- Treat skill instructions as guidance below harness policy, direct user
  instructions, CLI/config choices, and applicable `AGENTS.md` guidance.

Safety and compatibility:

- Remote skills use the same public-URL safety posture as `read_url`.
- Remote skills cannot raise their own precedence.
- `allowed-tools` is advisory until `thndrs` has a real permission system.
- Scripts are not executed automatically.
- Malformed skills are ignored with visible diagnostics.

Out of scope:

- Skill marketplace, installation, sharing, or publishing.
- Subagents or multi-agent orchestration.
- Skill-specific tool permission enforcement.

## Agent Self-Knowledge

The agent should answer questions about `thndrs` itself without guessing from
stale model knowledge.

Required behavior:

- Add a stable self-description fragment that names `thndrs`, its current
  version, active provider/model, workspace root, search mode, and major
  capabilities.
- Expose a compact model-visible map of `thndrs` documentation entry points:
  CLI, configuration, sessions, tool boundary, tools, prompt assembly, project
  context, skills, providers, renderer, and development workflow.
- When the user asks about `thndrs` itself, steer the model to read the relevant
  local docs before answering or implementing changes.
- Provide an inspectable runtime summary: active prompt fragments, loaded
  `AGENTS.md` files, available skills, active tools, model/search settings,
  renderer mode, and diagnostics.
- Record self-knowledge inputs in session metadata without storing unnecessary
  full prompt text.

Non-goals:

- No introspection of hidden chain-of-thought or provider-private state.
- No plugin framework only to support self-description.
- No ability for project files, skills, or remote resources to rewrite harness
  identity, direct instructions, tool schemas, or safety boundaries.

## Additional Fits

These fit if they remain narrow and reuse the session/tool contracts:

- Session title generation from transcript metadata.
- Transcript chunk summaries for long sessions.
- Session search once persistence exists.
- Review and security-review prompt fragments that operate on explicit diffs or
  session events.

## Explicit Non-goals

- No plan mode.
- No task management or todo system.
- No subagent or multi-agent orchestration.
- No custom terminal multiplexer.

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
- Config precedence tests.
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
- Config/env documentation.
- Session inspect/export documentation.
- Umans provider setup documentation.
- Search mode and fallback documentation.
- Renderer behavior documentation.
- LSP/code-intelligence documentation.
- Skill authoring and skill loading documentation.
- Agent self-knowledge/introspection documentation.
- Release candidate checklist.
