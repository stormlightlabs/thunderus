# thndrs-agent

`thndrs-agent` is an experimental, provider-neutral Rust library for authors of
coding-agent applications.

It provides a small vocabulary for iterations of agent loops, namely messages in,
semantic events out, tool requests, permission decisions, and cooperative cancellation.

Use it when your application owns the model client and tool policy but you want
the rest of the agent loop to speak small, typed Rust values.

A terminal UI, web service, or editor integration can all consume the same events
without importing a provider's wire types.

## Installation

Add it to your crate or workspace's `Cargo.toml`:

```toml
[dependencies]
thndrs-agent = "0.1"
serde_json = "1"
```

## Describe a Turn

Build an [`AgentTurn`] from your application's conversation and tool catalog,
then lower it in your provider adapter.

The provider adapter returns [`AgentEvent`] values as work progresses.

```rust
use thndrs_agent::{AgentMessage, AgentTurn, ToolDefinition};

let turn = AgentTurn::new(
    vec![AgentMessage::User("Summarize the README".into())],
    vec![ToolDefinition::new(
        "read_file",
        "Read a workspace file",
        serde_json::json!({"type": "object"}),
    )],
);

assert_eq!(turn.messages.len(), 1);
assert_eq!(turn.tools[0].name, "read_file");
```

[`AgentRun`] is a small helper for a background run. Give it a closure that
sends your event type and checks the supplied [`CancelToken`]. Then, consume
the receiver in the UI or transport that owns presentation.

## Control Context

[`context`] provides pure, deterministic context control for agent turns:

- typed candidate items
- token budgets
- selection
- compact ledgers
- compaction policy.

Hosts supply project instructions, transcript entries, skills, and pins.

## Modules

- [`adapters`](https://docs.rs/thndrs-agent/0.1.0/thndrs_agent/adapters/) contains application-owned permission and execution callbacks.
- [`budget`](https://docs.rs/thndrs-agent/0.1.0/thndrs_agent/budget/) bounds tool batches and continuation segments.
- [`cancel`](https://docs.rs/thndrs-agent/0.1.0/thndrs_agent/cancel/) provides a shared cooperative cancellation token.
- [`context`](https://docs.rs/thndrs-agent/0.1.0/thndrs_agent/context/) contains pure selection, control, and compaction policy.
- [`contracts`](https://docs.rs/thndrs-agent/0.1.0/thndrs_agent/contracts/) defines provider-neutral messages, events, tools, and retry values.
- [`run`](https://docs.rs/thndrs-agent/0.1.0/thndrs_agent/run/) owns a background run's event channel and cancellation handle.

## Keep Policy in your application

This crate does not read files, run processes, or contact a model provider.

Use [`ToolPermissionHook`] when your application must approve a sensitive tool
call, and [`ToolExecutionHook`] when it needs to take over execution. The
application context and result types remain yours.

[`AgentEvent`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/enum.AgentEvent.html
[`AgentRun`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.AgentRun.html
[`AgentTurn`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.AgentTurn.html
[`CancelToken`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.CancelToken.html
[`context`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/context/index.html
[`ToolExecutionHook`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.ToolExecutionHook.html
[`ToolPermissionHook`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.ToolPermissionHook.html
