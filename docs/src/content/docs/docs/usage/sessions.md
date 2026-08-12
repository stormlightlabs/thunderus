---
title: "Sessions"
description: "Resume, fork, inspect, retain, archive, and remove local thndrs sessions."
---

`thndrs` writes durable runs to append-only JSONL files in the selected session
directory. The workspace default is:

```text
.thndrs/sessions/session-YYYYMMDD-HHMMSS.jsonl
```

A session is the durable audit and resume record. The context sent to a model is
a selected projection of that record plus current project instructions, skills,
tools, and runtime metadata.

## Durable and ephemeral runs

Runs are durable by default. Use `--ephemeral` or `--no-session` when a run
should exist only in memory:

```sh
thndrs --ephemeral
thndrs run --ephemeral "inspect this workspace"
```

An ephemeral run creates no session JSONL, artifact bodies, per-session log, or
shared daily log. It cannot be resumed or renamed. Shared configuration and
prompt-input history keep their normal policies.

## Browse sessions

`session` and `sessions` are aliases. Session ids may be exact or unique
prefixes; ambiguous prefixes fail and print the matches.

```sh
thndrs sessions list
thndrs sessions latest
thndrs sessions titles
thndrs sessions show session-20260710
```

`list` scans live, archived, and trashed state and shows sessions newest first.
Its inventory includes title, model, last activity, parent, token totals,
storage state, pin state, lock state, corruption, and lineage diagnostics.
`show` prints replayable transcript entries. `latest` prints the newest session,
while `titles` provides a compact title list.

Inside the TUI, `/history` opens the session picker. It offers resume, archive,
unarchive, pin, unpin, delete, and restore actions where the session's current
state permits them. Deletion has a confirmation step.

## Resume, fork, and rename

Resume appends to the original session:

```sh
thndrs sessions resume session-20260710
```

Resume validates the record and acquires an exclusive writer lock. It restores
settled transcript entries, context-control state, compaction summaries, and
token totals needed for continuation. It does not restart tools or processes,
recreate a pending permission prompt, or rebuild an unsent input queue.

Fork when you want an independent continuation from a settled turn:

```sh
thndrs sessions fork session-20260710 turn-42
```

The child receives its own id and append-only file. Its `session_fork` record
names the parent turn and complete root-to-parent lineage. Missing parents,
malformed chains, and cycles appear as diagnostics without hiding unrelated
valid sessions.

Rename changes the display title, not session identity:

```sh
thndrs sessions rename session-20260710 "OAuth cleanup"
```

## Inspect and export

Inspection and export are read-only and never replay recorded actions:

```sh
thndrs sessions inspect session-20260710 --format json
thndrs sessions export session-20260710 --format jsonl > session.jsonl
thndrs sessions export session-20260710 --format markdown > session.md
thndrs sessions export session-20260710 --format html > session.html
```

`inspect` produces a stable renderer-independent projection. `export` supports:

- `jsonl`: redacted valid records in append-only sequence order;
- `json`: one JSON document;
- `markdown`: a bounded human review copy;
- `html`: a self-contained bounded review copy.

Human review exports organize messages, tool and shell activity, artifacts,
request accounting, context transformations, compactions, and lineage. Session
exporters cap their input and mark a truncated review copy. Raw provider
payloads, unrecorded live state, and full file bodies are not reconstructed.

Use `/context export` for the model-visible context projection of the active
session rather than the complete session record. See
[Context](/docs/usage/context/#export-the-current-projection).

## Archive and pin

Archiving removes a session from the live set without deleting its durable
state:

```sh
thndrs sessions archive <id>
thndrs sessions unarchive <id>
```

Pins protect live or archived sessions from automatic retention:

```sh
thndrs sessions pin <id>
thndrs sessions unpin <id>
```

A session pin is different from `/context pin`: the first protects a complete
stored session from retention; the second keeps one item in the current model
working set.

Lifecycle commands accept `--format human` or `--format json`.

## Delete, restore, and purge

`delete` is reversible. Without `--yes`, it prints the exact owned paths and
shared artifacts affected by the operation:

```sh
thndrs sessions delete <id>
thndrs sessions delete <id> --yes
thndrs sessions restore <id>
```

Deletion moves session-owned state to recoverable trash. Restore is available
only during `session_retention.trash_retention_days`. Active or locked sessions
cannot be changed. Pinned sessions require `--allow-pinned` in addition to the
normal confirmation.

`purge` previews or permanently removes eligible workspace-owned session state:

```sh
thndrs sessions purge
thndrs sessions purge --yes
```

Permanent deletion is irreversible. Shared artifacts remain while another live
or archived session references them.

## Retention and storage

Inspect storage and policy-reclaimable bytes with:

```sh
thndrs sessions storage
thndrs sessions storage --format json
```

Preview or apply retention to unprotected live sessions:

```sh
thndrs sessions prune --dry-run
thndrs sessions prune --older-than 14 --keep-count 50 --dry-run
thndrs sessions prune --older-than 14 --keep-count 50
```

Automatic retention is enabled by default. At most once every 24 hours, a
best-effort collection pass moves eligible unprotected live sessions to trash,
expires old trash, removes unreferenced artifacts and orphan state, and applies
log limits. It skips the active session, locks, pins, corrupt records, and
sessions younger than the configured minimum age.

See [Configuration](/docs/reference/configuration/#session-retention) for the
default age, live-count, and trash-grace settings.

## What a session stores

A durable session can contain:

- user prompts and finalized assistant and reasoning text;
- tool starts, arguments, statuses, capped outputs, and recovery artifacts;
- context ledgers and request-bound context snapshots;
- `AGENTS.md`, prompt, skill, and configuration metadata;
- compaction summaries, reviews, pins, drops, recovery, and lifecycle actions;
- request accounting and provider-reported usage;
- file-write, shell, MCP, ACP, permission, queue, failure, and cancellation
  audits.

The JSONL does not contain raw provider requests or responses. File-write audits
store paths, hashes, and byte counts instead of file bodies. Tool evidence and
artifact bodies are bounded and redacted before persistence.

## Storage graph and diagnostics

The inventory treats JSONL, locks, per-session logs, archive, trash, pins,
per-session state, and artifacts as one graph. Artifact bodies are measured from
filesystem metadata rather than loaded during inventory. A shared artifact is
counted once and retained until no retained session references it.

Missing or malformed sidecars, missing bodies, orphan state, unreferenced
artifacts, damaged records, and invalid lineage become diagnostics. Valid
sessions remain available when another session is damaged.

Read bounded redacted logs with:

```sh
thndrs debug tail --lines 100
thndrs debug session-log session-20260710 --lines 100
```

See [Session Format](/docs/reference/session-format/) for the record schema and
storage layout.
