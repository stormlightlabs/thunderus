---
title: Context Control: Agent Primitive and Memory Removal
status: Ready
captured: 2026-07-11
---

## Objective

Make context control a reusable, provider-neutral primitive of
`thndrs-agent`, while removing all built-in memory behavior from `thndrs`.

`thndrs` remains a lean coding-agent application: it assembles project and
session inputs, owns filesystem and terminal effects, and uses the agent
library's deterministic context policy to construct a turn. Durable memory,
retrieval, storage, indexing, and memory-specific commands belong to a future,
agent-agnostic product rather than this workspace.

## User Stories

- As a host application author, I can use `thndrs-agent` to budget, select,
  project, and compact an agent's context without importing provider, terminal,
  filesystem, or memory code.
- As a `thndrs` user, I retain the existing non-memory context behavior:
  scoped project instructions, token-aware prompt assembly, pins, transcript
  recovery, and compaction safeguards.
- As a `thndrs` user, I cannot accidentally enable or invoke an unfinished
  built-in memory system; no memory configuration, command, storage root, or
  session mutation remains.

## Current State

- `thndrs-context` contains both pure context policy and a file-backed Markdown
  memory implementation with SQLite FTS/BM25 retrieval.
- `thndrs-agent` owns provider-neutral turn and run contracts, but has no
  context module.
- `thndrs` is the only in-workspace consumer of `thndrs-context`. Its CLI/TUI,
  configuration, prompt/session code, tests, and public documentation still
  expose memory behavior.
- `thndrs` already has generic MCP client support. It is not a memory protocol
  and this work must neither extend nor remove it.
- The project notebook, including the Letta and memory research, is
  project-agnostic research and remains unchanged.

## Design

### Library boundary

The workspace has two reusable layers plus the application:

```text
thndrs-agent::context ──> thndrs (CLI/TUI and ACP server)
```

`thndrs-agent::context` owns only pure, provider-neutral context control:

- typed context items, visibility, diagnostics, ledgers, and token budgets;
- model-limit resolution, deterministic selection, prompt projection inputs,
  and stable item identifiers;
- pin, transcript, skill, instruction, harness, and compaction-summary
  candidate types;
- compaction configuration, risk classification, and preflight decisions.

`thndrs` owns application adapters and effects:

- workspace-root and `AGENTS.md` discovery, including file-size limits and
  filesystem diagnostics;
- application configuration parsing, provider request construction, session
  records, and recovery storage;
- CLI/TUI commands, focused surfaces, renderer projections, ACP transport, and
  generic MCP configuration/tools.

The move preserves non-memory selection, budget, prompt, and compaction
semantics. `SelectionInput`, `ContextItemKind`, prompt projection, and session
metadata lose every memory field or variant rather than retaining deprecated
placeholders.

### Clean memory removal

This unreleased workspace has no compatibility or migration obligation. Remove:

- the `thndrs-context` package, its feature flags, README, source tree, and
  workspace/dependency references;
- file-backed Markdown memory, FTS/BM25 indexing and recall, memory roots and
  caches, secret checks specific to memory writes, and related dependencies;
- memory configuration, environment/config provenance, diagnostics, CLI/TUI
  commands and help, prompt selection, session-record variants, audit writers,
  and tests;
- product and internal planning documentation that presents memory as a
  `thndrs` capability.

Old memory configuration and session data may fail rather than receiving a
legacy parser, migration, warning, or disabled-mode implementation. The
application must never scan, create, read, write, or index `.thndrs/memory`,
`~/.thndrs/memory`, or a memory cache after this work.

### Context-control interaction

The context-control feature continues without memory. It provides bounded,
redacted inspection and deliberate working-set controls for context, pins,
recovery, and compaction. `/doctor` reports source, pin, budget, and
compaction health only. Focused context surfaces remain legible at narrow and
short terminal sizes without replacing native transcript scrollback.

## Success Criteria

- `thndrs-agent::context` provides the moved pure policy without application,
  provider-wire, filesystem, terminal, ACP, or memory dependencies.
- `thndrs-context` and every in-process memory API, command, configuration
  value, session record, storage path, index, and product claim are absent.
- Existing non-memory context behavior remains covered by deterministic ledger,
  selection, compaction, prompt, CLI/TUI, and ACP regression tests.
- Context inspection, working-set mutation, compaction review, and health
  surfaces refer only to context state and preserve prompt input on failure.
- Generic MCP behavior is unchanged and no memory-specific MCP contract is
  introduced.
- Project-agnostic notebook material remains unchanged.

## Testing Plan

**Test boundary:** verify pure selection and compaction through
`thndrs-agent::context`; verify application behavior through the existing
CLI/TUI, prompt/session, renderer, and ACP test boundaries.

- Move and retain context ledger, selection, and compaction unit tests after
  deleting memory-only cases.
- Remove memory-only tests and verify that the supported application paths no
  longer create or inspect a memory root or cache.
- Preserve deterministic tests for instruction precedence, budget decisions,
  pins/drops/recovery, prompt projection, compaction review, and redacted
  session inspection.
- Snapshot new or changed context-only surfaces at normal, narrow, and
  small-height dimensions.
- Build the public documentation after memory product claims are removed.

```text
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test -p thndrs-agent
cargo test -p thndrs
cargo test --workspace
pnpm --dir docs build
```

## Boundaries

Always:

- preserve direct user and project-instruction precedence and existing context
  safety rules;
- keep context policy deterministic and side-effect free;
- preserve generic MCP and ACP behavior;
- remove memory code completely instead of replacing it with a disabled mode.

Ask first:

- creating a memory-engine repository, package, protocol, or public API;
- adding a dependency to `thndrs-agent` beyond what the moved pure context code
  requires;
- changing unrelated provider, session, permission, or MCP behavior.

Never:

- add a legacy memory parser, migration, or compatibility switch;
- scan or create a memory directory/cache;
- treat the project notebook as product documentation to delete or rewrite;
- create a memory-specific MCP contract in this workspace.

## Deferred Milestone

An agent-agnostic memory engine is a separate product. Its storage model,
permissions, CLI, MCP server contract, and integration behavior require their
own repository and specification once there is a concrete consumer. It does
not block this context-only refactor.

## Risks and Review Points

- The crate move is a broad but mechanical public-API change. Keep behavior
  stable through narrow tests and the complete workspace suite.
- Removing every memory reference spans source, configuration, session
  serialization, help, and documentation. Search the full workspace before
  declaring completion; distinguish memory as durable agent capability from
  ordinary in-memory prompt-history terminology.
- Review the final package graph and dependency lockfile to ensure no
  `thndrs-context`, SQLite, or memory-only dependency survives.

The detailed implementation frontier is in [tasks.md](tasks.md).
