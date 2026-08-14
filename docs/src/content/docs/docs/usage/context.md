---
title: "Context"
description: "Inspect and control what the model can see in a thndrs session."
---

Before every provider request, `thndrs` builds the context that the model can
see. This working set is a projection of session evidence, not the session
itself. Selection, compaction, or an explicit context action can remove older
records from the projection while preserving them in the append-only session
log.

Open the context overlay in the TUI with:

```text
/context
/context show
/context all
```

The overlay shows the projected budget and each candidate's id, kind, visibility,
lifecycle, selection reason, token estimate, protection state, and recovery availability.

For a shorter health report, use `/doctor`. It covers sources, discovery diagnostics,
pins, drops, model-limit provenance, the projected budget, and compaction review state.

## Context pressure and measured usage

`/context` estimates how much of the selected model's input limit the next
request will use. `/usage` reports provider-measured consumption for the latest
request and the session: input, output, reasoning, cache reads, cache writes,
request count, and cost when available.

Provider account capacity and its latest refresh state appear separately from
consumption. Missing measurements are shown as unknown. If a provider's input
total includes cached tokens but has no cache breakdown, fresh input also
remains unknown.

## Inspect persisted context history

The top-level commands read semantic records from the newest session by
default. Select another session with its exact id or a unique prefix:

```sh
thndrs context
thndrs context --session <id>
thndrs context changes
thndrs context changes <from-request> <to-request>
thndrs usage
thndrs usage --json --session <id>
```

Use `thndrs context --json` for a versioned context history containing request
snapshots, adjacent diffs, accounting, transformations, diagnostics, and
measurement provenance. The export carries its schema, policy, lineage,
redaction state, and size limits. It contains metadata and stable ids, not
retired request bodies or artifact bodies.

These commands do not reconstruct provider requests or replay session actions.
They reject histories and encoded exports that exceed their configured bounds.
Content-capture options are not available for this metadata-only export.

## What can enter the working set

The `kind` column identifies what each item represents:

- `harness`: system prompt fragments, environment metadata, and tool schemas
- `project_instruction`: root or nested `AGENTS.md` instructions
- `pinned_file`: a task-local pinned file, file range, tool result, or note
- `skill`: activated skill instructions or discovered skill metadata
- `transcript`: user, assistant, reasoning, or settled tool entries
- `summary`: an approved compaction summary for older transcript entries
- `tool_archive`: recoverable archived tool output or transcript content

Harness context and the current user turn are protected from ordinary budget
eviction. Pins take priority over ordinary transcript history. Applicable
project instructions and activated skills precede the recent transcript tail.
Under pressure, older transcript entries leave the projection first.

Kind describes the content. Visibility describes whether the item enters the
current request:

- `visible` and `pinned` items are rendered for the model
- `candidate` items were discovered but are not applicable or selected
- `summary_only` items are represented by an approved summary
- `archived` items are outside the active projection but may be recoverable
- `dropped` items were explicitly excluded
- `blocked` items could not fit as a single bounded item

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

Pins last for the session. Use project instructions or configuration for
permanent context. A pin remains visible until it is dropped or the session
ends.

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

Some items are protected because they contain a current constraint, pending
permission, failure evidence, recovery metadata, or an unverified write.
Release protection directly with:

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

`/compact` asks the selected model to summarize an older part of the
conversation. Once validated and approved, `thndrs` uses the summary instead of
those messages in later requests. The original records remain in the session.

Compaction is best effort because the model writes the summary. Validation can
catch structural problems, but it cannot guarantee that the summary preserves
every important detail from the original conversation.

When review is required, resolve it with:

```text
/context review
/context review approve
/context review reject
```

See [Context Compaction](/docs/concepts/context-compaction/) for automatic mode,
summary validation, and retained-tail behavior.

## Exporting the current projection

Export the selected context as versioned JSON or Markdown:

```text
/context export context.json
/context export context.md markdown
/context export context.json --artifacts
```

Relative paths resolve from the workspace. The export includes the budget,
ordered item metadata, bounded model projection, available request accounting,
reduction receipts, and diagnostics. Token estimates and provider measurements
include their provenance.

Artifact bodies are omitted unless `--artifacts` is present. Included bodies
and text fields are truncated and redacted. Raw provider payloads are never
exported.

This slash command exports the active model-visible projection. To inspect
request snapshots already recorded in a session, use `thndrs context --json`
instead.

## Shortening tool output

`thndrs` can shorten tool output before sending it back to the model. It can
remove terminal formatting and repeated progress updates, condense blank or
repeated lines, skip evidence when the underlying state has not changed, and
leave out oversized failed-tool arguments after saving recovery details.

The cleanup rules produce the same result for the same tool output.

By default, `thndrs` only measures what each rule would change and sends the
original output. Session records show which rules were measured or applied.

You can enable these rules individually. See [Configuration](/docs/reference/configuration/#context-reduction)
for the specific settings.

## Related documentation

- [Project Context](/docs/usage/project-context/)
- [Sessions](/docs/usage/sessions/)
- [Prompt Assembly](/docs/concepts/prompt-assembly/)
- [Session Format](/docs/reference/session-format/)
