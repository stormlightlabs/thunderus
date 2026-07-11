---
title: thndrs Baseline Tickets
status: Ready
captured: 2026-07-11
---

The baseline foundation is complete. These completed outcomes define the
current starting point for later feature work.

## Establish the two-crate workspace

**What was built:** A `thndrs-agent` library and the `thndrs` application
package, with unchanged human-only release authority.

**Acceptance criteria:**

- [x] The workspace contains only `thndrs-agent` and `thndrs`.
- [x] Package metadata and executable ownership remain in `thndrs`.

## Provide a reusable agent and context primitive

**What was built:** Provider-neutral agent contracts, background runs, and
pure context policy in `thndrs-agent::context`.

**Acceptance criteria:**

- [x] Context control has no provider-wire, terminal, filesystem, ACP, or
      durable-storage dependency.
- [x] Application adapters supply instruction discovery, persistence, and
      rendering.

## Compose the application safely

**What was built:** CLI/TUI and ACP adapters using the shared agent/context
primitive, append-only session inspection, local-tool safeguards, and generic
MCP support.

**Acceptance criteria:**

- [x] The fake provider, prompt, session, renderer, and ACP boundaries have
      regression coverage.
- [x] No in-process durable-memory capability, configuration, command, or
      compatibility path remains.

**Verification:**

- `cargo test --workspace`
- `pnpm --dir docs build`
