---
title: "Context"
description: "Inspect and control the model-visible working set for a thndrs session."
---

`thndrs` builds a fresh context working set for every provider request. The
working set is a projection of session evidence, not the session itself. Older
records can leave the projection because of selection, compaction, or an
explicit context action while remaining in the append-only session log.

Open the context overlay in the TUI with:

```text
/context
/context show
/context all
```

The overlay reports the current budget and each candidate's id, kind,
visibility, lifecycle, selection reason, token estimate, protection state, and
recovery availability. `/doctor` prints a shorter health report covering
sources, discovery diagnostics, pins, drops, model-limit provenance, budget,
and compaction review state.

## What can enter the working set

The context ledger tracks:

- harness prompt fragments and tool schemas;
- the current user turn and recent settled transcript entries;
- applicable root and nested `AGENTS.md` instructions;
- activated skills and discovered skill metadata;
- task-local pinned files or evidence;
- the latest approved compaction summary;
- archived tool or transcript evidence with a recovery handle.

Harness context and the current user turn are protected from ordinary budget
eviction. Pins are considered before ordinary transcript history. Applicable
project instructions and activated skills precede the recent transcript tail.
When the request is under pressure, older transcript entries leave the
projection first.

A context item's visibility describes the current request:

- `visible` and `pinned` items are rendered for the model;
- `candidate` items were discovered but are not applicable or selected;
- `summary_only` items are represented by an approved summary;
- `archived` items are outside the active projection but may be recoverable;
- `dropped` items were explicitly excluded;
- `blocked` items could not fit as a single bounded item.

Visibility is separate from lifecycle. An item may be active, duplicate,
superseded, summarized, or archived across requests. Lifecycle records explain
what replaced an item and whether a user reviewed the relation.

## Inspect one item

Use the stable id shown by `/context`:

```text
/context item <id>
```

The result includes the origin, kind, lifecycle, visibility and reason, token
estimate, artifact handle, protection state, and recovery availability. Item
ids are also present in context exports and `context_ledger` session records.

## Pin, drop, and recover

Pin a path or an existing context id when it must stay in the task-local working
set:

```text
/context pin src/auth.rs
/context pin <id>
```

Pins are session context, not a permanent project configuration mechanism. A
pin is rendered until it is dropped or the session ends.

Drop an item when it is irrelevant to the current task:

```text
/context drop <id>
/context drop --reset
```

A dropped source remains excluded until its source changes or drops are reset.
Dropping context does not delete its source file or remove its session audit
record.

Recover an archived or recoverable item by id or handle:

```text
/context recover <id-or-handle>
```

Recovery uses bounded, redacted evidence. It does not replay a tool or restore
a running process.

## Protected evidence and verification

Some items carry protection reasons such as a current constraint, pending
permission, failure evidence, recovery metadata, or an unverified write. A
direct release is explicit:

```text
/context release <id>
```

For a reviewable verification relation, propose the candidate that verifies a
protected item, then approve or reject the returned relation id:

```text
/context verify propose <protected-id> <candidate-id>
/context verify approve <relation-id>
/context verify reject <relation-id>
/context verify release <relation-id>
```

A proposal alone does not release protection. Approval records the review;
`release` applies the approved verification and releases the associated
protection.

## Compaction

`/compact` asks the selected model to summarize a closed prefix of the
conversation. A validated, approved summary replaces that prefix only in later
model projections. The source records remain in the session.

When review is required, resolve it with:

```text
/context review
/context review approve
/context review reject
```

See [Context Compaction](/docs/concepts/context-compaction/) for automatic mode,
summary validation, and retained-tail behavior.

## Export the current projection

Export the selected context as versioned JSON or Markdown:

```text
/context export context.json
/context export context.md markdown
/context export context.json --artifacts
```

Relative paths resolve from the workspace. The export includes the budget,
ordered item metadata, bounded model projection, request accounting when
available, reduction receipts, and diagnostics. Artifact bodies are omitted
unless `--artifacts` is present. Included bodies and text fields are bounded and
redacted; raw provider payloads are never exported.

## Deterministic projection reduction

Compaction is model-generated and operates on conversation ranges. Projection
reducers are local, deterministic transformations of bounded tool evidence.
They can clean terminal controls and progress redraws, normalize blank runs,
collapse consecutive repeated lines, suppress state-identical evidence, project
structured command results, or omit oversized failed-tool arguments after
recovery evidence has been persisted.

Reducers are independent and disabled for application by default. Shadow
measurement is enabled by default, so inspection can report what a reducer
would have changed without altering the request. Applied and shadow decisions
are recorded as reduction receipts.

See [Configuration](/docs/reference/configuration/#context-reduction) for the
switches.

## Related documentation

- [Project Context](/docs/usage/project-context/)
- [Sessions](/docs/usage/sessions/)
- [Prompt Assembly](/docs/concepts/prompt-assembly/)
- [Session Format](/docs/reference/session-format/)
