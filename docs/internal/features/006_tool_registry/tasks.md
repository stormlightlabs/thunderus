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

- [ ] Move `read_file_range` schema into its module.
- [ ] Move `find_files` schema into its module.
- [ ] Move `list_searchable_files` schema into its module.
- [ ] Move `search_text` schema into its module.
- [ ] Move `sawk` schema into its module.
- [ ] Move argument parsing into each module.
- [ ] Add per-tool parse tests.
- [ ] Add per-tool execution tests through the registry.

## P3: Move Search And URL Tools

- [ ] Move `web_search` schema into the search tool boundary.
- [ ] Move `read_url` schema into the URL-read tool boundary.
- [ ] Preserve network/public URL safety checks.
- [ ] Preserve output caps and truncation metadata.
- [ ] Add registry execution tests for success and safety failures.

## P4: Move Write Tools

- [ ] Move `create_file` schema into its module.
- [ ] Move `replace_range` schema into its module.
- [ ] Move `write_patch` schema into its module.
- [ ] Preserve hash guards and atomic failure behavior.
- [ ] Preserve `WriteResult` side effect metadata.
- [ ] Add registry execution tests for successful writes.
- [ ] Add registry execution tests for stale hashes and path escapes.
- [ ] Add session tests proving write side effects are recorded.

## P5: Move Shell Tool

- [ ] Move `run_shell` schema into `shell` module.
- [ ] Preserve argv-only execution.
- [ ] Preserve working-directory containment.
- [ ] Preserve timeout and truncation behavior.
- [ ] Preserve background process registration.
- [ ] Preserve shell audit metadata.
- [ ] Add registry execution tests for foreground commands.
- [ ] Add registry execution tests for background commands.
- [ ] Add session tests proving shell side effects are recorded.

## P6: Delete Central Dispatch

- [ ] Replace the central match with registry lookup.
- [ ] Remove duplicate schema literals from `src/tools.rs`.
- [ ] Remove stale helper code that only supported the central match.
- [ ] Ensure provider prompt assembly uses the derived catalog.
- [ ] Ensure fake, Umans, Anthropic, OpenAI-compatible, and OpenCode provider
      paths see identical tool definitions.
- [ ] Update prompt snapshots.

## P7: Docs

- [ ] Update internal tool-boundary docs.
- [ ] Update public tool reference if schema descriptions or examples change.
- [ ] Document how to add a built-in tool.
- [ ] Document side-effect audit behavior for write and shell tools.
- [ ] Document why MCP enters through the external-tool path in `008_mcp`.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [ ] `cargo test tools`
- [ ] `cargo test prompt`
- [ ] `cargo test session`
- [ ] `cargo test`
