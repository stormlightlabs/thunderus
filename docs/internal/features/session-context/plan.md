# Session Context

`ContextLedger` is the source of truth for candidate inventory, request
selection, budgets, and transformations. Context views, session records, status,
transcript events, and exports project ledger data. UI, telemetry, and provider
adapters neither recalculate accounting nor expose raw provider payloads.

## Context surfaces

`/context` describes the next provider request. Its summary shows projected
input used and available, percentage remaining, target and auto-compaction
thresholds, estimate provenance, and model-limit source and confidence. It
groups selected and available context into Harness, Instructions, Skills,
Pinned, Conversation, Summaries, and Tool results. It also counts selected,
omitted, recoverable, and protected items.

- `/context` shows the next request summary.
- `/context all` lists up to 64 current items.
- `/context changes` compares adjacent requests or two chosen requests.
- `/context item <id>` shows an item's origin, lifecycle, inclusion or omission
  reason, estimate, artifact handle, protection, and recovery path.

Historical views name the request, turn, and attempt they describe and label the
data as a snapshot rather than a live projection.

An optional `context-remaining` status segment renders a plain value such as
`ctx 73% left`. The sparse default omits it. The segment reads the `/context`
projection, hides when the value is unknown or space is tight, and shows neither
a gauge nor cumulative usage. Crossing the target or auto-compaction threshold
triggers a temporary warning.

## Durable request history

Extend the current ledger session record to persist a content-free
`ContextSnapshot` for every provider request attempt. Each snapshot identifies
the request, turn, attempt, model route, limits, budget, candidate and selected
counts, projected usage, optional provider usage, transformations, and
diagnostics. Store aggregates and stable item references or changes so item
bodies are not copied into every snapshot.

`ContextDiff` compares snapshots through stable item IDs. It records additions,
removals, lifecycle changes, replacements, reductions, compactions, recoveries,
and instruction-scope changes. Each change carries before and after measurements
and their provenance. Compaction changes also record the reclaimed amount,
retained recent range, summary reference, source range, and recovery details.

The transcript renders compaction, tool-output reduction, and recovery as
context events. These entries link to the request and inspection details, and
the transcript does not treat them as tool calls or assistant content.

A request detail view combines the snapshot with request timing, tool activity,
and provider usage. It shows the model and route, duration and time to first
token, projected and measured input, fresh and cached input when reported,
output and reasoning tokens, tool count and duration, and context changes.

## Accounting boundaries

- Context pressure estimates one request's projected input against a model
  limit.
- Usage measures cumulative provider consumption and cost.
- Account capacity reports provider allowances such as rate windows or credits.

`/context` owns context pressure. `/usage` owns request and session consumption.
Provider account capacity appears under a separate label. Every value retains
its estimate or measurement provenance, and missing provider data displays as
unknown rather than zero.

## Session lifecycle

Context compaction changes a provider request. Session retention changes durable
local state. Keep the two policies separate and place session lifecycle in the
`thndrs` application adapters, where workspace storage, logs, and terminal
interaction already live.

Durable runs write session JSONL, referenced artifact bodies, and a per-session
log. `--ephemeral`, with `--no-session` as an alias, keeps the run in memory and
does not create those files or write the shared daily log. Shared settings and
prompt history keep their own storage policies. Tests and automation use
ephemeral runs unless persistence is the behavior under test.

Treat a session as a storage graph rather than one JSONL file. Its graph includes
the session record, per-session log, artifact references, and future checkpoints,
plans, task state, or temporary attachments. A fork may reference artifacts also
used by its parent, so deletion removes an artifact body only when no retained
session references it. Shared daily logs are not session-owned and follow an
independent log policy.

Lifecycle terms have distinct meanings:

- Archive hides a session from the default browser without reclaiming storage.
- Delete moves one session graph to application trash. It can be restored during
  the grace period; permanent deletion is explicit.
- Prune selects deletable sessions from a retention policy and supports a dry
  run before applying it.
- Purge deletes all eligible session state for one workspace after a scoped
  preview and confirmation.
- Garbage collection removes expired trash, unreferenced artifacts, stale
  temporary files, and other state that cannot be reached from retained
  sessions.

The initial automatic retention policy is enabled, uses a 30-day maximum age and
200-session cap for unprotected live sessions, never removes a session less than
one day old, and retains trash for seven days. Age comes from the last durable
session activity rather than filesystem modification time. A session is
eligible when it exceeds either the age or count limit. The minimum age wins
during bursts. Active or locked sessions and pinned sessions are protected from
pruning; a title does not implicitly pin a session. An explicit delete can
remove an archived or pinned session after confirmation, but cannot remove a
session in use.

Run automatic collection opportunistically at startup or resume when the last
successful pass is at least 24 hours old. Use the same pure selection logic as
the prune preview, skip live locks, apply changes atomically where practical,
and record the policy, time, reclaimed bytes, and skipped failures. Cleanup is
best effort and must not delay or fail an agent run. Corrupt sessions stay in
place for inspection, and uncertain reachability prevents automatic artifact
deletion.

`thndrs session storage` reports live, archived, pinned, trash, artifact, and log
counts and bytes, including how much the current policy could reclaim. The
session browser exposes search, archive, pin, delete, and storage details using
the same inventory. Per-session logs follow their session into trash. Shared
daily logs use independent age and size caps so diagnostics cannot grow without
bound.

## Sessions, exports, and privacy

Fork a session only at a replayable settled turn. Record its lineage and copy a
self-contained semantic prefix. Leave pending tools, permissions, queues,
processes, and other live runtime state behind. Session browsing reports broken
parents, malformed lineage, cycles, locks, and corruption.

One versioned snapshot and diff model feeds the TUI, session JSONL,
non-interactive CLI, Markdown, and JSON exports. Exports use deterministic
ordering, provider-neutral fields, redaction, and configured item and artifact
limits.

Persist metadata only by default. Retaining normalized request content or
artifact bodies requires a per-run opt-in and approved retention, access,
deletion, and size rules. Session deletion, pruning, purge, and garbage
collection apply those rules to captured content as part of the same storage
graph. Capture excludes credentials and raw provider wire payloads.

After local snapshots and exports ship, an optional OpenTelemetry exporter can
read the persisted records. It emits low-cardinality counts, timings, token
measurements, and transformation events. It includes content only when the
request-content capture policy allows it.
