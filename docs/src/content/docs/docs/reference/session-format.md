---
title: "Session Format"
---

Durable sessions are append-only JSONL. Each line is one record with
`schema_version`, a session-local monotonic `seq`, an ISO 8601 UTC `time`, and a
`type` tag. The current schema version is `1`. Appends are the only record-file
mutation.

## Identity and lineage records

- `session_meta`: id, workspace, title, provider, model, search backend, app
  version, and redacted effective-configuration provenance.
- `session_renamed`: a new display title; the latest record wins.
- `session_fork`: direct parent, settled parent turn, and the complete
  root-to-parent lineage.
- `acp_session`: separate local and external ACP session identities plus
  redacted agent and protocol metadata.

A fork starts with its own `session_meta` followed by one `session_fork` record.
Inventory validates retained lineage. Missing parents, malformed chains, and
cycles become diagnostics; they do not invalidate unrelated sessions.

## Transcript and request records

- `user`: prompt text and turn id.
- `assistant_finished`: finalized replayable assistant text.
- `reasoning_finished`: finalized replayable reasoning text.
- `cancelled` and `failed`: terminal turn state and reason.
- `usage`: provider token-usage increment.
- `request_accounting`: provider-neutral serialized request measurements,
  model projection metadata, reduction receipts, and normalized provider usage.
- `prompt_metadata`: content-free prompt assembly metadata for one turn.

Streaming deltas are not session records. Only settled transcript entries are
replayable after resume. Raw provider request and response payloads are not
persisted by default.

## Context records

- `context`: path, scope, hash, byte count, and truncation state for loaded
  instruction sources.
- `context_ledger`: all candidate item metadata, visibility and lifecycle,
  budget, model-limit provenance, aggregate projection, and diagnostics for a
  prompt turn.
- `context_snapshot`: a versioned request-bound ledger for one provider attempt.
  Append-only states report dispatch, completion, failure, or interruption and
  can add serialized bytes, estimated input, transformations, and provider
  usage as they become known.
- `context_pin`, `context_drop`, and `context_recovery`: explicit working-set
  actions with content-free item metadata and a redacted reason.
- `context_lifecycle`: duplicate, supersession, summary, archive, recovery,
  verification, protection, and release actions.
- `compaction`: validated approved range summary, covered source sequences and
  hashes, prior-summary ids, protected facts, recovery handles, review state,
  local estimates, provider usage when known, and context-edit diagnostics.
- `compaction_review`: approval or rejection of a pending summary, keyed by its
  recovery handle.

Context metadata contains labels, paths, hashes, sizes, token estimates, states,
and handles. It does not duplicate full `AGENTS.md` text or raw context bodies.
A rejected compaction writes `compaction_review` but no `compaction` record. A
successful compaction does not remove the covered records.

## Tool and side-effect records

- `tool_started`: turn, call id, name, arguments, and optional MCP identity.
- `tool_finished`: call id, status, capped redacted output, optional artifact,
  and optional MCP identity.
- `file_write`: operation, path, before and after hashes and byte counts, and
  status. It does not store file contents.
- `shell_exec`: command, cwd, process id when backgrounded, lifecycle status,
  exit code, elapsed time, and one-shot/background kind.
- `mcp_config_changed`: previous and current MCP config file hashes plus loader
  diagnostics.
- `skill_activated`: source and rendered hashes and sizes plus loaded reference
  metadata.
- `queued_input`: queue id, steering/follow-up kind, lifecycle action, and text.
- `acp_permission_request` and `acp_permission_outcome`: redacted permission
  options and the selected or cancelled result.

One-shot commands have one terminal `shell_exec` record. Background commands
have a `running` record and a later terminal record. Full stdout and stderr are
not stored in `shell_exec`; bounded redacted output belongs to tool evidence.
Queue records are audit history. Resume does not recreate an unsent queue after
a crash.

## Artifacts and recovery

Completed tool evidence may reference artifact metadata and a stable handle.
Artifact bodies live outside JSONL, are bounded and redacted before persistence,
and may be shared by multiple sessions. Context recovery and export can use a
handle without replaying the original tool.

## Storage graph

For the default workspace layout, session-owned and shared state is organized
as follows:

```text
.thndrs/sessions/<id>.jsonl
.thndrs/sessions/<id>.jsonl.lock
.thndrs/sessions/artifacts/<handle>.json
.thndrs/sessions/artifacts/<handle>.body
.thndrs/sessions/archive/
.thndrs/sessions/trash/
.thndrs/sessions/pins/
.thndrs/sessions/state/
.thndrs/logs/sessions/thndrs-<id>.log
```

Archive and trash move session-owned state; pins and state use sidecars.
Inventory scans the graph without loading artifact bodies. Shared artifacts are
counted once and remain until no live or archived session references them.
Missing sidecars or bodies, orphan state, unreferenced artifacts, and malformed
records produce diagnostics rather than hiding valid sessions.

## Read, inspect, and export behavior

Readers preserve every valid record after a malformed line. Session inspection
and export redact secret-looking values, bound their output, and never execute a
stored action.

```sh
thndrs sessions inspect <id> --format json
thndrs sessions export <id> --format jsonl
thndrs sessions export <id> --format markdown
thndrs sessions export <id> --format html
```

JSONL export preserves valid record sequence order. JSON inspection is a stable
renderer-independent projection. Markdown and HTML exports are bounded semantic
review copies rather than byte-for-byte replicas of the JSONL.

See [Sessions](/docs/usage/sessions/) for lifecycle and retention commands, and
[Context](/docs/usage/context/) for working-set inspection and export.
