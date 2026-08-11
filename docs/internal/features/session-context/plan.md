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

- `/context` shows the summary and changes since the last request.
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
deletion, and size rules. Capture excludes credentials and raw provider wire
payloads.

After local snapshots and exports ship, an optional OpenTelemetry exporter can
read the persisted records. It emits low-cardinality counts, timings, token
measurements, and transformation events. It includes content only when the
request-content capture policy allows it.
