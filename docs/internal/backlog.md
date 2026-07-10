# Internal Backlog

This file is only for unowned or cross-cutting backlog. If a feature folder
tracks the work, keep the detail there instead of duplicating it here.

## Architecture Refactors

### Baseline-Owned Agent Boundary

The provider-neutral agent event boundary and application composition seam are
tracked in [`000_baseline`](features/000_baseline/plan.md). The tool-registry
migration itself is complete and archived in `archive/v0.1.md`.

### App State Boundaries

- Split `App` state into narrower sub-state for UI/editor state, session
  recording, process registry, auth recovery, MCP audit state, and permissions.
- Preserve the single `update(&mut App, Msg)` mutation path.
- Prefer small moves that reduce field coupling before introducing traits or
  effect systems.

### Input Handling

- Move raw key handling toward command primitives.
- Translate terminal keys to small input/app command enums before mutating
  `App`.
- Cover mode-specific key translation with tests.

## Search And File Discovery

- Prefer `fd` for repository file discovery.
- Prefer `rg --json` for content search.
- If `fd` is missing, use a bounded fallback that preserves workspace
  containment and ignores common generated/vendor directories.
- If `rg` is missing, use a bounded fallback that preserves output caps and
  marks results as degraded.
- Surface fallback choice in diagnostics, transcript/tool metadata, and tests.

## LSP And Code Intelligence

Add read-only LSP-backed tools only when they are clearly better than plain file
search.

Supported operations:

- document symbols;
- workspace symbols;
- go to definition;
- find references;
- hover;
- find implementations where supported.

Rules:

- never edit files through LSP;
- prefer installed language servers and degrade clearly when none is available;
- keep startup and indexing bounded with visible diagnostics;
- record LSP calls as structured transcript/tool events;
- preserve plain file search as fallback;
- unit-test fixture responses and no-server fallback behavior;
- add snapshots for LSP transcript entries.

## Required Checks

- Prompt input tests for grapheme-aware insert, delete, backspace, cursor
  movement, word movement, wrapping, and cursor placement.
- Long-prompt benchmark or spike covering `String` versus Ropey before adopting
  a rope-backed prompt buffer.
- LSP fixture tests and no-server fallback tests.
- Skill metadata validation and progressive-loading tests.
- Remote skill fetch safety and reference-depth tests.
- Agent self-knowledge snapshot tests.
- Local `cargo package`.
- Local install-path smoke test.

## Release Artifacts

- `CHANGELOG.md` using Keep a Changelog categories.
- Install documentation.
- Search mode and fallback documentation.
- LSP/code-intelligence documentation.
- Skill authoring and skill loading documentation.
- Agent self-knowledge/introspection documentation.
- Release candidate checklist.

## Parking Lot

- Tool call failures should have more debuggable logs and transcript context.
- Plan mode.
- In-app task management.
- Subagents or multi-agent orchestration.
- Custom terminal multiplexer.
- LSP code actions or automatic refactors.
- Long-lived LSP server process management.
- Skill marketplace, installer, sharing, or publishing.
- Skill-specific tool permission enforcement.
- Plugin framework for self-description.
- Provider-private state introspection.
- Guardrails for project files, skills, or remote resources rewriting harness
  identity, direct instructions, tool schemas, or safety boundaries.
