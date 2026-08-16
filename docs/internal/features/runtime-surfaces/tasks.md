# Runtime Surface Tasks

## RUN-1: Complete headless JSONL runs

- [x] Require an exact model route, absolute contained cwd, session policy,
      settings, authority, task, timeout, and evidence and resource limits.
- [x] Reject traversal, implicit model or workspace defaults, invalid state
      transitions, and unbounded requests before provider work starts.
- [x] Give each execution a run identity distinct from its durable session
      identity.
- [x] Emit starting, ready, running, waiting for permission, cancelling,
      succeeded, failed, and cancelled states through valid transitions.
- [x] Preserve exact model identifiers through configuration, events, session
      metadata, and results.
- [x] Return bounded semantic evidence, changed-file metadata, and a durable
      session handle.
- [x] Keep stdout protocol-clean and diagnostics bounded and redacted.
- [x] Preserve distinct errors for unsupported models, missing credentials,
      invalid workspaces, protocol mismatch, provider failures, cancellation,
      and timeout.
- [x] Keep existing callers compatible when they ignore new optional metadata.
- [x] Leave scheduling, hierarchy, dependencies, and concurrency to callers.
- [x] Exercise the same application run path as interactive execution rather
      than reimplementing lifecycle or result rules in the JSONL adapter.

## RUN-2: Ship ACP as a stable packaged surface

**Blocked by:** None - can start immediately.

- [ ] Support initialization, streaming, tools, permissions, cancellation,
      session settlement, resume, and close through the packaged command.
- [ ] Preserve exact route, model, workspace, session, authority, lifecycle,
      result, and error semantics when translating ACP messages.
- [ ] Use the application run path shared with interactive and headless
      execution; keep ACP-specific state and wire messages in the adapter.
- [ ] Make registry and discovery material name the real command and supported
      capabilities.
- [ ] Resolve assets and configuration identically in packaged and source
      execution.
- [ ] Keep stdio as the only transport until a deployment requires another.
