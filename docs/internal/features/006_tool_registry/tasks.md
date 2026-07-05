# Tool Registry Tasks

Status: Draft
Captured: 2026-07-04

## P0: Define The Registry Contract

- [x] Define `ToolContext`.
- [x] Define `ToolExecution` or equivalent unified result type.
- [x] Define `ToolError`.
- [x] Define the registry entry shape.
- [x] Define how schemas are converted for each provider.
- [x] Define how structured side effects flow to session records.
- [x] Define stable test examples for every existing tool.
- [x] Existing `thndrs` tool names stay stable during the registry migration.
- [x] Old `read`, `write`, `edit`, `bash`, and `research` names are not
      compatibility aliases.
- [x] MCP enters through `008_mcp` after the built-in registry exists.
- [x] Memory tools enter through `001_context_control` after the memory storage
      and retrieval contract is implemented.

## P1: Add Registry Around Existing Tools

- [x] Add registry module-level docs.
- [x] Add unique-name validation.
- [x] Derive `tool_definitions()` from the registry.
- [x] Keep old dispatch temporarily behind registry lookup.
- [x] Add snapshot test for the derived catalog.
- [x] Add tests proving names are stable and unique.
- [x] Add tests proving schemas are JSON objects with expected required fields.

## P2: Move Read-Only Tools

- [x] Move `read_file_range` schema into its module.
- [x] Move `find_files` schema into its module.
- [x] Move `list_searchable_files` schema into its module.
- [x] Move `search_text` schema into its module.
- [x] Move `sawk` schema into its module.
- [x] Move argument parsing into each module.
- [x] Add per-tool parse tests.
- [x] Add per-tool execution tests through the registry.

## P3: Move Search And URL Tools

- [x] Move `web_search` schema into the search tool boundary.
- [x] Move `read_url` schema into the URL-read tool boundary.
- [x] Preserve network/public URL safety checks.
- [x] Preserve output caps and truncation metadata.
- [x] Add registry execution tests for success and safety failures.

## P4: Move Write Tools

- [x] Move `create_file` schema into its module.
- [x] Move `replace_range` schema into its module.
- [x] Move `write_patch` schema into its module.
- [x] Preserve hash guards and atomic failure behavior.
- [x] Preserve `WriteResult` side effect metadata.
- [x] Add registry execution tests for successful writes.
- [x] Add registry execution tests for stale hashes and path escapes.
- [x] Add session tests proving write side effects are recorded.

## P5: Move Shell Tool

- [x] Move `run_shell` schema into `shell` module.
- [x] Preserve argv-only execution.
- [x] Preserve working-directory containment.
- [x] Preserve timeout and truncation behavior.
- [x] Preserve background process registration.
- [x] Preserve shell audit metadata.
- [x] Add registry execution tests for foreground commands.
- [x] Add registry execution tests for background commands.
- [x] Add session tests proving shell side effects are recorded.

## P6: Delete Central Dispatch

- [x] Replace the central match with registry lookup.
- [x] Remove duplicate schema literals from `src/tools.rs`.
- [x] Remove stale helper code that only supported the central match.
- [x] Ensure provider prompt assembly uses the derived catalog.
- [x] Ensure fake, Umans, Anthropic, OpenAI-compatible, and OpenCode provider
      paths see identical tool definitions.
- [x] Update prompt snapshots.

## P7: Docs

- [x] Update internal tool-boundary docs.
- [x] Update public tool reference if schema descriptions or examples change.
- [x] Document how to add a built-in tool.
- [x] Document side-effect audit behavior for write and shell tools.
- [x] Document why MCP enters through the external-tool path in `008_mcp`.

## Validation Commands

- [x] `cargo fmt`
- [x] `cargo clippy --fix --allow-dirty --allow-staged`
- [x] `cargo clippy`
- [x] `cargo test tools`
- [x] `cargo test prompt`
- [x] `cargo test session`
- [x] `cargo test`
