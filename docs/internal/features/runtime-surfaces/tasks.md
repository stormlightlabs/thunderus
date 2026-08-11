# Runtime Surface Tasks

## RUN-1: Define the run contract

- [ ] Validate exact model route, absolute contained cwd, session policy,
      settings, authority, task, timeout, and evidence and resource limits.
- [ ] Define starting, ready, running, waiting for permission, cancelling,
      succeeded, failed, and cancelled states with valid transitions.
- [ ] Reject traversal, implicit model or workspace defaults, invalid
      transitions, and unbounded specifications locally.
- [ ] Return bounded semantic evidence, changed-file metadata, and a durable
      session handle.
- [ ] Keep scheduling, hierarchy, dependencies, and concurrency out of the
      contract.

## RUN-2: Unify identity and lifecycle across surfaces

**Blocked by:** RUN-1.

- [ ] Carry exact run, route, model, workspace, session, authority, lifecycle,
      and result semantics through TUI, JSONL, and ACP adapters.
- [ ] Preserve exact model identifiers through configuration, events, session
      metadata, and results.
- [ ] Keep stdout protocol-clean and diagnostics bounded and redacted.
- [ ] Preserve distinct errors for unsupported models, missing credentials,
      invalid workspaces, protocol mismatch, provider failures, cancellation,
      and timeout.
- [ ] Keep existing callers compatible when they ignore new optional metadata.

## RUN-3: Ship ACP as a stable packaged surface

**Blocked by:** RUN-2.

- [ ] Support initialization, streaming, tools, permissions, cancellation,
      session settlement, resume, and close through the packaged command.
- [ ] Make registry and discovery material name the real command and supported
      capabilities.
- [ ] Resolve assets and configuration identically in packaged and source
      execution.
- [ ] Keep stdio as the only transport until a deployment requires another.
