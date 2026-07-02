---
title: "Architecture"
---

## Module Map

```sh
src
├── agent.rs        # Fake and Umans agent event loop
├── app.rs          # App state, messages, update logic, state tests
├── cli.rs          # CLI args, value enums, parse tests
├── lib.rs          # Terminal setup and app run loop
├── main.rs         # Binary entrypoint
├── providers       # Concrete provider clients
├── renderer        # Row model, live region, backend, highlighting, snapshots
├── tools.rs        # Read-only repository tools
└── session.rs      # Append-only session persistence
```

## App State

The app starts with one `App` struct and plain enums. Shared state stays in
`App`; rendering details live in `src/renderer/`.

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

The direct renderer writes stable transcript blocks once into native terminal
scrollback. The live region redraws only active streaming content, dynamic
status, prompt input, accessory rows, and static footer status. A renderer-owned
row model keeps wrapping, padding, styling, and cursor placement testable
without terminal I/O.

## Provider Client

Umans is implemented as a concrete provider client. A generic provider trait is
deferred until there is a second provider.

## Tool Dispatch

Tool dispatch exposes typed, bounded read-only repository tools instead of raw
shell command strings.
