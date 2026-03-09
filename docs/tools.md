# Tool System Guide

This document describes how tools are registered, exposed to models, executed, and returned back into the conversation loop.

## Current Tool Inventory

Thunderus currently exposes seven runtime tools:

| Tool            | Category | Defined In                   | Executed In                   |
| --------------- | -------- | ---------------------------- | ----------------------------- |
| `read`          | base     | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |
| `write`         | base     | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |
| `edit`          | base     | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |
| `bash`          | base     | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |
| `research`      | base     | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |
| `memory_store`  | memory   | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |
| `memory_recall` | memory   | `crates/tools/src/schema.rs` | `crates/tools/src/runtime.rs` |

There is no query-based `web_search` function tool in runtime today. Web/document retrieval is handled by `research` (URL fetch + extraction).

## End-to-End Tool Flow

1. System prompt assembly:
    - `crates/core/src/prompt.rs` builds one system prompt from:
        - `meta/PROMPT.txt`
        - `meta/RESPONSE.txt`
        - `meta/TOOLS.txt`
2. Tool schema assembly:
    - `thndrs_tools::get_tool_schemas()` is called in `crates/providers/src/conversation_loop.rs`.
    - Core tools are read from `meta/TOOLS.txt` (`core_tools_from_meta()`), then memory tools are appended.
3. Provider serialization:
    - `ToolDefinition::from_schema(...)` converts each tool to provider wire format.
    - Current provider implementation is OpenAI-compatible function calling.
4. Model tool call:
    - Provider returns tool calls with `id`, `name`, and parsed JSON arguments.
5. Runtime execution:
    - `thndrs_tools::execute_tool(...)` dispatches by tool name in `crates/tools/src/lib.rs`.
6. Tool result return:
    - Runtime returns `ToolResult { status, content }`.
    - Conversation loop wraps this into provider payload and sends it back as a tool message.
7. Iteration:
    - Loop repeats until model returns final assistant content (or max tool iterations reached).

## Source Of Truth

For tool setup, each file has a specific responsibility:

- `meta/TOOLS.txt`: human-readable tool list shown in the system prompt.
- `crates/tools/src/schema.rs`: actual JSON Schema and tool descriptions sent to the model.
- `crates/tools/src/lib.rs`: registry + dispatch (`get_tool_schemas`, `execute_tool`).
- `crates/tools/src/runtime.rs`: tool behavior, sandbox controls, limits, and error text.
- `crates/providers/src/conversation_loop.rs`: orchestration of model calls and tool results.

Keep these in sync whenever adding/removing/renaming tools.

## Result Envelope

All runtime tool results are normalized to:

```json
{
  "status": "success" | "error",
  "content": "string",
  "tool_use_id": "string (optional)"
}
```

`tool_use_id` is populated when the conversation loop binds the result to a specific model-emitted tool call ID.

## Safety And Limits

Safety controls are implemented in `crates/tools/src/runtime.rs`:

- Path sandboxing for file tools (`ToolContext::resolve_path`).
- Read limits: text reads capped at 2000 lines per call.
- Write limits: content capped at 10 MB.
- Shell limits: `bash` timeout 120 seconds, output capped at 100 KB.
- Research limits: HTTPS only, private-host checks, 30-second timeout, 50 KB output cap.

## Adding A New Tool

1. Add schema constructor in `crates/tools/src/schema.rs`.
2. Add runtime executor in `crates/tools/src/runtime.rs`.
3. Register dispatch arm in `crates/tools/src/lib.rs::execute_tool`.
4. Include it in `get_tool_schemas()`.
5. Add docs in `docs/base-tools.md` (or the appropriate tool doc).
6. Update `meta/TOOLS.txt` and `meta/PROMPT.txt` guidance.
7. Add/extend tests:
    - schema tests in `crates/tools/src/schema.rs`
    - runtime tests in `crates/tools/src/runtime.rs`
    - conversation-loop schema conversion tests in `crates/providers/src/conversation_loop.rs`

## Verification Commands

From repo root:

```bash
cargo test -p tools
cargo test -p core
```

Use `cargo check --workspace` for a broader compile check.
