---
title: "Sessions"
---

`thndrs` writes each run to an append-only JSONL session file in the selected
workspace:

```text
.thndrs/sessions/session-YYYYMMDD-HHMMSS.jsonl
```

Sessions record replayable transcript entries, tool starts and finishes, loaded
project-context metadata, token usage, file-write audit metadata, and shell
execution metadata.

Runs are durable by default. Use `--ephemeral` (or its alias, `--no-session`)
for a run that should exist only in memory:

```sh
thndrs --ephemeral
thndrs run --ephemeral "inspect this workspace"
```

An ephemeral run does not create session JSONL, artifact bodies, a per-session
log, or the shared daily log. It cannot resume or name a session. Shared
settings and prompt history continue to follow their normal policies.

## What Is Stored

- User prompts and finalized assistant/reasoning text.
- Tool names, arguments, statuses, and capped outputs.
- `AGENTS.md` paths, scopes, content hashes, byte counts, and truncation state.
- File write operation, path, before/after hashes, byte counts, and status.
- Shell command, working directory, exit status, elapsed time, and process kind.

## What Is Not Stored

Session metadata does not store full raw provider payloads by default. File-write
records do not store full file contents. Shell stdout and stderr are capped and
redacted before being recorded through tool output.

## Recovery

Use normal repository tools such as `git diff` for file recovery and review. The
session file explains what tool ran, what file was targeted, and whether an edit
or command succeeded.

## Browse and Resume

Both `session` and `sessions` name the same CLI command group.

Session ids accept an exact id or a unique prefix. An ambiguous prefix is rejected
with the matching ids rather than picking one arbitrarily. Lists are newest first
and show the title, model, last recorded activity, fork source, token totals, and
lock, corruption, or lineage state.

```sh
thndrs sessions list
thndrs sessions show session-20260710-120000
thndrs sessions resume session-20260710
thndrs sessions fork session-20260710 turn-42
```

A fork starts a new session from a replayable settled turn. Its lineage records
the parent session and exact source turn. Missing parents, malformed lineage,
and cycles are reported in browsing surfaces without hiding valid sessions.

`resume` acquires an exclusive local writer lock before it can append.

It restores the saved transcript and token totals, but never restores a
running tool, pending permission, queued input, or other live process state.

Inside the TUI, use `/history`, `/session <id>`, `/resume <id>`, `/context`,
and `/tokens` to take a look at your usage.

`/context` shows item state, policy reason, protection, replacement,
and recovery availability.

`/tokens` distinguishes the local estimate from provider-reported components and
normalized input.

Export the current bounded context projection with `/context export`:

```text
/context export context.json
/context export context.md markdown
/context export context.json --artifacts
```

Exports are versioned and redact sensitive information.

Artifact bodies are excluded unless `--artifacts` is explicitly supplied.

Unselected source content and unredacted provider payloads are never exported.

The read-only commands leave the prompt draft intact when lookup or file reading fails.

## Inspection and export

`inspect` produces a stable, renderer-independent JSON projection. `export`
emits one redacted JSON record per line, in durable sequence order.
Neither operation replays a tool, compaction, or any other side effect.

```sh
thndrs sessions inspect session-20260710 --format json
thndrs sessions export session-20260710 --format jsonl > session.jsonl
```

The projection includes transcript, tool audit, usage, context, skill, and
configuration evidence already stored in the session.

It omits raw provider payloads, file bodies, and unrecorded live state.
Secret values are redacted (best-effort) in inspection and export output.

## Storage

The application treats session storage as one graph. It includes live session
JSONL and lock files, per-session logs, artifact metadata and bodies, and the
reserved archive, trash, pin, and per-session state locations. This inventory
collects counts, bytes, recorded age, archive and pin state, and reclaimable
bytes without reading artifact bodies.

Artifact references are followed across forks. A shared artifact remains
retained while any live or archived session references it and is counted once.
Missing, malformed, shared, and unreferenced storage is reported as diagnostic
state; it does not make unrelated valid sessions inaccessible.

## Debug logs

Diagnostic readers are bounded and redact secret-looking values:

```sh
thndrs debug tail --lines 100
thndrs debug session-log session-20260710 --lines 100
```

`/debug log [session-id]` reads the corresponding per-session log in the TUI.
