---
outline: deep
---

# System

Thunderus is a TUI-first agent harness. The UI owns the event loop and user experience.
The agent runs as a task that orchestrates model calls, tool execution, approvals, and
optional memory retrieval.

## High-Level Model

- **UI owns control**: input, cancellation, pause, and drift handling.
- **Agent owns orchestration**: builds requests, streams provider output, and mediates
  tools and approvals.
- **Providers stream**: tokens and tool calls, not full responses.
- **Sessions are audit logs**: append-only events for replay and inspection.

## Where to go deeper

- Runtime and tool flow details: [Data Flow](/development/data-flow)
- Crate and boundary map: [Architecture](/development/architecture)
- Conventions and patterns: [Patterns](/development/patterns)
