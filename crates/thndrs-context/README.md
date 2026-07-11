# thndrs-context

`thndrs-context` is for authors of coding-agent applications who need to build
a bounded, explainable prompt context. It provides pure context selection,
`AGENTS.md` discovery, content-free session metadata, and optional file-backed
memory.

Use it when your application needs to decide which instructions, transcript
entries, pins, and memory belong in a turn without tying that decision to a
particular model provider or user interface.

## Add it

```toml
[dependencies]
thndrs-context = "0.1"
```

Add the `memory` feature only when the application offers file-backed memory:

```toml
thndrs-context = { version = "0.1", features = ["memory"] }
```

## Select context for a turn

An application gathers candidates at its filesystem, session, and provider
boundaries. [`select_context`] then returns a [`ContextLedger`] containing the
selected items, the token budget, and any diagnostics. The same input produces
the same ledger.

```rust
use thndrs_context::context::{
    ModelContextLimits, SelectionInput, UserTurnCandidate, select_context,
};

let (limits, diagnostics) = ModelContextLimits::resolve("example", "model", None, None);
assert_eq!(diagnostics.len(), 1); // No provider metadata: use the conservative fallback.

let input = SelectionInput {
    user_turn: Some(UserTurnCandidate::new("session-1", 1, "Explain this crate".len())),
    ..SelectionInput::default()
};
let ledger = select_context(&input, limits);

assert_eq!(ledger.rendered().len(), 1);
```

The [`context`] module also exposes `AGENTS.md` discovery and instruction
selection. Treat instruction files as guidance: your application must keep
permissions, tool policy, and provider settings at its own boundary.

## Memory and session records

The default feature set does not include file-backed memory or lexical recall.
Enabling the `memory` Cargo feature makes those APIs available; it does not
decide when an application reads, indexes, retrieves, or writes memory. Keep
that runtime decision explicit in your application.

Use [`session`] types to persist context and memory evidence without copying
the model-visible body text into an audit record.

## Stability and boundaries

The crate is pre-1.0, so its public API can change before a 1.0 release. It has
no dependency on a provider client, terminal UI, or `thndrs-agent`;
applications supply those pieces themselves.

[`context`]: https://docs.rs/thndrs-context/latest/thndrs_context/context/index.html
[`ContextLedger`]: https://docs.rs/thndrs-context/latest/thndrs_context/context/struct.ContextLedger.html
[`memory`]: https://docs.rs/thndrs-context/latest/thndrs_context/memory/index.html
[`select_context`]: https://docs.rs/thndrs-context/latest/thndrs_context/context/fn.select_context.html
[`session`]: https://docs.rs/thndrs-context/latest/thndrs_context/session/index.html
