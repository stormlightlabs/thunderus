---
title: "Architecture"
---

## Module Map

```sh
. (crates/thndrs/src)
├── cli/             # CLI commands, app state, input, and renderer
│   └─ renderer/     # Row model, semantic views, bounded adapter, live region
├── core/            # Agent loop, providers, tools, prompts, auth, and sessions
├── server/          # ACP/server transport and session handling
├── lib.rs           # Library exports and terminal-facing composition
└── main.rs          # Binary entrypoint
```

## App State

The app starts with one `App` struct and plain enums.

Shared state stays in `App` while rendering details live in the `renderer` module.

## Messages

User input, ticks, submit, clear, quit, and provider/tool events are represented
as typed messages.

## Agent Events

Agent events include started, assistant delta, reasoning delta, tool started,
tool output, tool finished, finished, and failed.

## Update Loop

`update(&mut App, Msg) -> Option<Msg>` is the main mutation path. Follow-up
messages keep derived behavior inside the state machine instead of scattering
it through the terminal loop.

## UI Rendering

The renderer keeps the shell's normal terminal screen. Settled transcript rows
are appended to native scrollback, while a small live region at the bottom holds
streaming output, the prompt, status, pickers, and detail surfaces. The terminal
owns scrollback, mouse selection, search, and the blinking input cursor.

The renderer-owned row model keeps wrapping, padding, styling, and cursor
placement testable without terminal I/O. Resizes redraw only the bounded live
region; committed output stays under terminal ownership.

## Provider Client

ChatGPT Codex, OpenCode Zen, and OpenCode Go are concrete provider clients behind a small
streaming
provider trait. The agent loop derives one tool catalog from the registry and
passes it to each provider request. Anthropic-compatible routes receive
`name`/`description`/`input_schema` entries; OpenAI-compatible routes convert
the same catalog to function tools at the provider boundary.

## Tool Registry

Tool dispatch exposes typed, bounded tools instead of raw shell command strings.
Built-in tools live in `thndrs_core::tools::<tool>` and are registered in `thndrs_core::tools::registry`.

Each registry entry has a stable name, provider-visible definition, executor,
and valid example input.

To add a built-in tool:

1. Add a `thndrs_core::tools::<name>` module with module docs, a `definition()`,
   input parsing, `execute_request()`, and focused unit tests.
2. Register it in `thndrs_core::tools::registry` with its name, definition
   function, executor, and example input.
3. Add schema, parsing, execution, and failure tests. If it writes files or
   launches processes, return structured side-effect metadata and add session
   tests for the audit records.
4. Update the public tool reference when the model-visible name, fields,
   behavior, or examples change.
5. Run format, clippy, prompt/tool snapshot tests, relevant session tests, and
   the full test suite.

MCP tools are intentionally not added as built-ins. They enter through the
external-tool path with namespaced names and separate discovery/configuration,
while sharing the registry execution result shape and audit behavior.
