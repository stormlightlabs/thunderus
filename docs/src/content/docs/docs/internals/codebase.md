---
title: "Codebase Tour"
---

This page maps architectural responsibilities to the crates and modules that
own them. Start here when you know what needs to change but not where the
change belongs.

## Workspace Map

```text
.
├── Cargo.toml                 # workspace and shared dependency versions
├── crates/
│   ├── thndrs-agent/          # provider-neutral agent library
│   │   ├── src/               # contracts, runs, context policy, accounting
│   │   ├── tests/             # library behavior tests
│   │   └── fixtures/          # replay and context fixtures
│   └── thndrs/                # application, CLI, TUI, and ACP server
│       ├── src/
│       ├── tests/             # application and ACP integration tests
│       └── snapshots/         # Ratatui and transcript snapshots
└── docs/                      # Astro documentation site
```

`thndrs-agent` is a reusable leaf library. It does not discover files, store
sessions, read the terminal, or know provider wire formats. `thndrs` composes
that library with those application adapters and exposes the resulting CLI,
terminal interface, and ACP server.

## Application Modules

```text
crates/thndrs/src
├── cli/
│   ├── commands/              # clap command definitions and command adapters
│   ├── app/                   # interactive state machine and app-owned state
│   ├── input/                 # terminal input, keymap, history, and actions
│   └── renderer/              # projections, transcript rows, live surfaces,
│                              # layout, styles, and Ratatui drawing
├── core/
│   ├── agent/                 # provider-backed run orchestration
│   ├── acp/                   # ACP-facing configuration, events, and runner
│   ├── config/                # local configuration loading and editing
│   ├── context/               # workspace instructions and context discovery
│   ├── mcp/                   # MCP configuration, catalog, and connections
│   ├── prompt/                # prompt fragments, templates, and assembly
│   ├── providers/             # provider adapters and streaming normalization
│   ├── session/               # append-only JSONL persistence and retention
│   ├── tools/                 # built-in tool registry, projection, and dispatch
│   ├── harness/               # UI-independent application harness entrypoint
│   ├── skills.rs              # skill discovery and activation
│   ├── artifacts.rs           # bounded application-owned tool artifacts
│   ├── auth.rs, trust.rs      # credentials and authority decisions
│   └── review.rs, diagnostics.rs,
│       internals.rs, utils/   # supporting application features
├── runtime/                   # command routing and terminal event loop
├── server/                    # ACP stdio transport and server sessions
├── headless.rs                # non-interactive application support
├── lib.rs                     # public composition and library entrypoint
└── main.rs                    # binary entrypoint
```

The `core` modules are the shared application layer. The TUI and ACP server
should use this layer rather than calling each other. `runtime` owns the
interactive terminal loop; `server` owns ACP transport and protocol handlers.

## Agent Library Modules

The public modules in `crates/thndrs-agent/src` have deliberately separate
responsibilities:

- `contracts.rs` defines provider-neutral messages, tool definitions, tool
  results, permissions, and agent events.
- `run.rs`, `adapters.rs`, and `cancel.rs` provide background run delivery,
  tool hooks, and cooperative cancellation.
- `context/` contains deterministic context selection, lifecycle, compaction,
  reduction, and state-deduplication policy. Hosts supply discovered inputs and
  decide how to persist or render the result.
- `accounting.rs` measures model projections and normalizes provider usage;
  `budget.rs` owns tool-iteration limits.
- `instances.rs` defines bounded child-instance contracts and outcomes.
- `replay.rs` loads fixtures and compares projection or recovery policies.

The crate root (`lib.rs`) re-exports the contracts and the main run, context,
accounting, instance, and replay types. Provider adapters and application
transcript types do not cross this public boundary.

## Find Code by Responsibility

| Responsibility                               | Start here                                        |
| -------------------------------------------- | ------------------------------------------------- |
| Provider-neutral events and tool contracts   | `crates/thndrs-agent/src/contracts.rs`            |
| Context policy and model projection          | `crates/thndrs-agent/src/context/`                |
| Background run delivery and cancellation     | `crates/thndrs-agent/src/run.rs` and `cancel.rs`  |
| Workspace instructions and context discovery | `crates/thndrs/src/core/context/`                 |
| Prompt templates and final prompt assembly   | `crates/thndrs/src/core/prompt/`                  |
| Provider authentication and streaming        | `crates/thndrs/src/core/providers/` and `auth.rs` |
| Agent run orchestration                      | `crates/thndrs/src/core/agent/`                   |
| Built-in tools and their registry            | `crates/thndrs/src/core/tools/`                   |
| MCP tools and server connections             | `crates/thndrs/src/core/mcp/`                     |
| Session files, records, and retention        | `crates/thndrs/src/core/session/`                 |
| Interactive application state                | `crates/thndrs/src/cli/app/`                      |
| Terminal input and key bindings              | `crates/thndrs/src/cli/input/` and `input.rs`     |
| Transcript and live rendering                | `crates/thndrs/src/cli/renderer/`                 |
| Command and terminal-loop routing            | `crates/thndrs/src/runtime/`                      |
| ACP transport and protocol sessions          | `crates/thndrs/src/server/`                       |

## Where Should New Code Live?

Use the narrowest owning layer:

- Put reusable run contracts, context policy, accounting, or cancellation in
  `thndrs-agent`. The code must remain independent of filesystems, terminals,
  sessions, and provider payloads.
- Put provider-specific request conversion, authentication, stream parsing,
  retry classification, and error messages in `core/providers/<provider>.rs`.
- Put workspace discovery, configuration, prompts, MCP, persistence, and
  built-in tool behavior in the matching `core/` module.
- Put command-line parsing and command-specific filesystem or process adapters
  in `cli/commands/`.
- Put state transitions in `cli/app/`. Put terminal event polling and effect
  execution in `runtime/`, not in the renderer.
- Put projection and drawing code in `cli/renderer/`. A renderer should read
  application state and produce rows or frames; it should not own agent state.
- Put ACP protocol conversion and stdio handling in `server/`. Reuse the shared
  core and harness instead of adding a second agent loop.

If a change needs both a reusable policy and an application adapter, keep the
policy in `thndrs-agent` and pass the adapter's data into it. This keeps the
library testable and prevents application concerns from leaking into public
library APIs.

## Dependency Boundaries

The dependency direction is:

```text
terminal / ACP frontend
          │
          ▼
  thndrs application core
          │
          ▼
     thndrs-agent
```

The application core may depend on `thndrs-agent`. The library must not depend
on `thndrs`, Ratatui, ACP, provider SDK payloads, or application session
records. Provider modules may implement the application-side
`StreamingProvider` boundary, but provider continuation data stays private to a
run and is not a session or library contract.

The TUI and ACP server are sibling frontends. `cli/app`, `runtime`, and
`renderer` own terminal behavior; `server` owns ACP transport. Shared behavior
belongs in `core` or `thndrs-agent`, not in a frontend module.

Session persistence is also an application boundary. Session records can store
normalized events, tool audits, context metadata, and redacted projections;
they must not become a way to expose raw provider wire payloads through the
library API.

## Source Map

| Concept                              | Primary source                                                  |
| ------------------------------------ | --------------------------------------------------------------- |
| Library public surface               | `crates/thndrs-agent/src/lib.rs`                                |
| Library contracts                    | `crates/thndrs-agent/src/contracts.rs`                          |
| Context policy                       | `crates/thndrs-agent/src/context/mod.rs`                        |
| Application public composition       | `crates/thndrs/src/lib.rs`                                      |
| Shared core module registry          | `crates/thndrs/src/core/mod.rs`                                 |
| Provider-neutral application harness | `crates/thndrs/src/core/harness/mod.rs`                         |
| Agent/provider orchestration         | `crates/thndrs/src/core/agent.rs` and `agent/`                  |
| Built-in tool boundary               | `crates/thndrs/src/core/tools.rs` and `tools/registry.rs`       |
| Session schema and writer            | `crates/thndrs/src/core/session/mod.rs` and `session/writer.rs` |
| Interactive state machine            | `crates/thndrs/src/cli/app.rs` and `cli/app/`                   |
| Terminal loop                        | `crates/thndrs/src/runtime/mod.rs`                              |
| ACP server entrypoint                | `crates/thndrs/src/server/mod.rs`                               |

## Related

- [Architecture overview](/docs/internals/)
- [Runtime and state](/docs/internals/runtime/)
- [Request lifecycle](/docs/internals/lifecycle/)
- [Context assembly](/docs/internals/context/)
- [Development workflow](/docs/development/workflow/)
- [Adding a tool](/docs/development/adding-a-tool/)
