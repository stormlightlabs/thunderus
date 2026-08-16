# Runtime Surfaces

`thndrs` runs the same provider-neutral application flow through the TUI,
JSONL, and ACP adapters. Headless input names the exact model route, absolute
contained workspace, session policy, settings, authority, resource bounds,
and task. Callers own parent IDs, delegation depth, concurrency groups, and
other orchestration topology.

A run identity names one execution; a session identity names durable
conversational context. Lifecycle and terminal-result semantics stay the same
across surfaces even though JSONL and ACP keep their own wire formats. Results
contain bounded summaries, evidence, changed-file metadata when applicable,
and a durable session handle rather than an unbounded transcript.

The packaged `thndrs acp serve` command must behave like source execution and
be discoverable by ACP clients. Stdio remains the transport until a concrete
deployment requires another one. Protocol stdout stays clean, while bounded
redacted diagnostics distinguish configuration, credentials, workspace,
protocol, provider, cancellation, and timeout failures.
