---
title: "Context Compaction"
description: "How thndrs reduces model-visible conversation history while retaining an append-only session record."
---

Long-running work can outgrow a model's usable input budget. Context compaction reduces
the conversation sent to the model so work can continue but does not rewrite or discard
the session that records the work.

## Two views of a session

`thndrs` maintains two related views:

- The canonical session is an append-only JSONL record of user messages, assistant output,
  tool activity, and context decisions. Compaction records describe approved summaries and
  the source range they cover.
- The model-facing projection is the selected context assembled for the next request.
  It includes durable prompt material, the active compaction summary, a recent verbatim
  transcript tail, and the current user turn.

Compaction changes the second view only. The original records remain available for session
inspection, export, and bounded recovery. Project instructions, skills, tools, and other
durable context are rebuilt for each request rather than delegated to a conversational
summary.

## When compaction runs

Run `/compact` while the TUI is idle to compact a closed prefix of the current conversation.
The selected completion model generates the summary as an internal request. That request is
not added to prompt history or the user transcript.

In automatic mode, thndrs estimates the full request before sending the user's turn. If
it exceeds the configured share of the available input budget, compaction runs first.
The budget accounts for the selected model's context window and completion reserve. After
a successful compaction, thndrs rebuilds the projection and restarts the original user
turn. If compaction cannot complete, the original turn and prior projection are preserved
so the user can retry or edit the turn.

## Closed history and the recent tail

A summary only replaces a closed prefix. thndrs chooses the boundary at the start of a
user turn, then retains that turn and everything after it verbatim. An assistant response
and its tool activity therefore remain together in the recent tail rather than being split
across a summary boundary.

`keep_recent_tokens` is an approximate target, not a hard cut through a turn. The tail can
be larger when the nearest safe user-turn boundary requires it. An explicit `/compact` can
still summarize a small closed history when there is no large tail to retain.

## Anchored summaries

The first compaction summarizes its covered history. Later compactions do not ask the model
to repeatedly summarize the entire session, but instead provide the previous summary as an
anchor and add only the newly closed transcript range. The next summary supersedes the earlier
active summary while retaining its provenance.

Summaries use a typed, versioned continuation format. It records the current objective,
findings, decisions, relevant paths, failures, verification, blockers, protected facts,
and source metadata. This gives the next model request structured continuation state
instead of an unbounded transcript.

## Compaction prompt

The internal request uses this prompt template. Braced fields are filled from the selected
closed range and its active summary anchor.

```plaintext
Summarize the closed context range for continuation. Return JSON only, matching this exact schema: {"schema_version":1,"objective":string,"findings":[string],"decisions":[string],"paths":[string],"failures":[string],"verification":[string],"blockers":[string],"protected_facts":[{"source_id":string,"text":string}],"sources":[{"sequence":number,"id":string,"content_hash":number,"recovery_handle":string}],"source_summary_ids":[string]}. Do not invent task state. Copy every protected fact, source record, and source-summary id exactly.

Focus: {focus}
Sources: {source_metadata}
Protected facts: {protected_metadata}
Source summaries: {source_summary_ids}

<source_context>
{source_text}
</source_context>
```

## Validation and review

A generated summary must pass local validation before it can affect the model-facing projection.
thndrs checks its schema, objective, source sequences and metadata, earlier-summary references,
and every protected fact required from the source range. A malformed or incomplete response
leaves the prior projection in place.

Some ranges need a person to inspect the proposed replacement. With the default review
policy, summaries covering tool output or diffs, failures, permission state, corrections, or
unresolved work wait for `/context review approve` or `/context review reject`. Approval
writes the compaction audit record and activates the new projection. Rejection records the
review decision and leaves the prior projection intact.

## Lifecycle

A compaction follows this sequence:

1. Select a closed transcript range while retaining a complete recent tail.
2. Send the selected model an internal request containing the prior anchor, if any,
   and the newly closed source range.
3. Validate the typed response and classify the source range for review.
4. Apply required review.
5. Write the approved audit record, activate the summary in later model projections,
   and restart the pending user turn for automatic compaction.

The session can later restore its active context state from those records. The audit
includes the covered range, recovery handles, summary provenance, review outcome, and
local size estimates.

## Configuration and commands

Configure the compaction mode, review policy, automatic threshold, and retained-tail
target under `[context.compaction]`.

### Further Reading

See [Configuration](/docs/reference/configuration/#context-compaction) for the supported
settings and defaults.

See the [CLI reference](/docs/reference/cli/) for `/compact`, `/context`, and context
review commands. [Token Optimization](/docs/concepts/token-optimization/) explains how
semantic compaction relates to deterministic reduction, and
[Session Format](/docs/reference/session-format/#compaction-records) documents the audit record.
