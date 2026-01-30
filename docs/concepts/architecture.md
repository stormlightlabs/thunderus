---
outline: deep
---

# Architecture

Thunderus is a Cargo workspace split into focused crates. Conceptually, the
system is a layered stack: lower crates define core types and capabilities;
higher crates compose behavior and UI.

## Crate Overview (Conceptual)

```text
core       # Types, approvals, memory, session
providers  # LLM adapters
tools      # Tool framework + builtins
store      # Memory persistence
skills     # Plugin system
agent      # Orchestration loop
ui         # TUI rendering
cli        # Binary entry point
```

Lower crates do not depend on higher crates. The UI owns the event loop.

## Where to go deeper

- Detailed crate responsibilities and boundaries: [Development Architecture](/development/architecture)
- Runtime request path: [Data Flow](/development/data-flow)
