# thndrs-agent

`thndrs-agent` is for authors of coding-agent applications who need a small,
provider-independent vocabulary for one turn. This includes messages in, semantic
events out, tool requests, permission decisions, and cancellation.

Use it when your application owns the model client and tool policy but you want
the rest of the agent loop to speak stable, typed Rust values. A terminal UI,
web service, or editor integration can all consume the same events without
importing a provider's wire types.

## Add it

```toml
[dependencies]
thndrs-agent = "0.1"
serde_json = "1"
```

## Describe a turn

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
sends your event type and checks the supplied [`CancelToken`]; consume the
receiver in the UI or transport that owns presentation.

## Control context

[`context`] provides pure, deterministic context control for agent turns:
typed candidate items, token budgets, selection, compact ledgers, and
compaction policy. Hosts supply project instructions, transcript entries,
skills, and pins; the module performs no filesystem, provider, terminal, or
persistence work.

## Keep policy in your application

This crate does not read files, run processes, or contact a model provider.
Use [`ToolPermissionHook`] when your application must approve a sensitive tool
call, and [`ToolExecutionHook`] when it needs to take over execution. The
application context and result types remain yours.

## Stability

The crate is pre-1.0. Expect its public API to evolve while `thndrs` and other
consumers establish the boundaries worth committing to.

[`AgentEvent`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/enum.AgentEvent.html
[`AgentRun`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.AgentRun.html
[`AgentTurn`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.AgentTurn.html
[`CancelToken`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.CancelToken.html
[`context`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/context/index.html
[`ToolExecutionHook`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.ToolExecutionHook.html
[`ToolPermissionHook`]: https://docs.rs/thndrs-agent/latest/thndrs_agent/struct.ToolPermissionHook.html
