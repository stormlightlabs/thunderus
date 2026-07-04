# Tool Registry Plan

Status: Draft
Captured: 2026-07-04

## Problem

The current tool layer has these risks:

- tool schemas and dispatch are manually synchronized;
- tool input validation is spread across dispatch and modules;
- session side effects for file writes and shell executions are special-cased;
- adding `memory_store`, `memory_recall`, LSP, or MCP tools will enlarge the
  central match;
- tests cannot easily prove that every schema has an executor and every
  executor has a schema.

## Milestone Outcome

Every built-in tool is an executable registry entry. Each tool module owns its
model-visible definition, input parsing, execution, output mapping, and side
effect metadata. The public tool catalog is derived from the registry.

The old tool names are not ported. Current `thndrs` tool names remain unless a
separate compatibility decision is made.

## Goals

1. Replace the central tool catalog with a registry of typed tool entries.
2. Move each built-in tool schema into the module that executes it.
3. Keep provider-facing schemas stable and snapshot-tested.
4. Keep safety rules visible in each tool module.
5. Preserve structured side effects for file writes and shell executions.
6. Make memory, LSP, and MCP tools additive instead of central-match changes.
7. Avoid a plugin framework in this feature.

## Registry Shape

Use a small static registry:

```rust
pub trait ToolSpec {
    const NAME: &'static str;

    fn definition() -> ToolDefinition;
    fn parse(arguments: &str) -> Result<Self::Input, ToolError>;
    fn execute(input: Self::Input, ctx: ToolContext) -> ToolExecution;
}
```

The exact Rust shape can change, but the boundary should preserve:

- stable tool name;
- schema;
- input parsing;
- execution;
- output;
- side effects;
- session audit metadata.

Avoid dynamic dispatch unless it removes real complexity. A static array of
function pointers or enum-backed entries is enough.

## Tool Execution Result

Unify tool execution output:

- display/model output lines;
- status;
- optional user-facing error;
- optional write audit;
- optional shell process audit;
- optional future memory audit;
- optional future external/MCP audit.

The session writer should consume this structured result without knowing which
tool produced it.

## Migration Order

1. Add registry types around the existing catalog.
2. Move one simple read-only tool, likely `read_file_range`, as a pattern.
3. Move remaining read-only filesystem tools.
4. Move URL/search tools.
5. Move write tools.
6. Move shell execution last because it has the richest side effects.
7. Delete the old central dispatch match when all tools are registered.

## Decisions

- Existing `thndrs` tool names stay stable during the registry migration.
- The old `read`, `write`, `edit`, `bash`, and `research` tool names are not
  compatibility aliases.
- MCP tools enter through `008_mcp` after the built-in registry exists.
- Memory tools enter through `001_context_control` after the memory storage and
  retrieval contract is implemented.
- Workspace containment, output caps, timeouts, and redaction remain mandatory
  registry-level invariants.
- The registry covers built-in and external tool entries. It is not a general
  plugin framework.
- Provider protocol changes are tracked in provider-normalization work; this
  feature consumes the derived catalog.

## Dependencies

- Current tool modules in `src/tools/`.
- Session writer side-effect records.
- Memory work in `001_context_control` and external tools in `008_mcp`.

## Verification

- Snapshot the full tool catalog.
- Test every registered tool has a unique name.
- Test every registered tool has a valid JSON schema.
- Test every registered tool can parse valid example input.
- Test invalid input produces stable errors.
- Test file write and shell side effects still reach session records.
