# Sessions

Status: Draft
Captured: 2026-07-04

## Problem

Current sessions are durable but underused:

- users cannot list sessions from the TUI or CLI;
- users cannot resume by id from a command surface;
- session summaries exist in code but are not a complete public interface;
- runtime logs exist but do not have a discoverable reader;
- inspect/export is on the roadmap but needs a focused contract;
- command suggestions currently omit history, resume, tokens, and debug
  surfaces that existed in the old project.

This makes session persistence feel like an implementation detail instead of a
working user feature.

## Milestone Outcome

A user can start `thndrs`, list recent sessions, resume a specific session,
inspect or export a session without opening the TUI, and read recent runtime
logs without knowing the on-disk layout.

The implementation keeps append-only JSONL as the source of truth. Any indexes
or summaries are derived and rebuildable.

## Goals

1. Add a small session command surface in the TUI and CLI.
2. Keep session replay independent of renderer internals.
3. Make inspect/export output stable enough for tests and future migration.
4. Expose runtime logs through bounded readers.
5. Preserve redaction, output caps, and machine-specific path policy.
6. Keep this feature focused on operations around existing session files.

## Public Contract

### TUI Commands

Supported command-mode commands:

- `history`: list recent sessions.
- `resume <id>`: load a saved session into the transcript.
- `session <id>`: show a compact session summary.
- `tokens`: show current session token totals.
- `debug log [session-id]`: show recent session log lines.

The same names are accepted through `/` submission while the app is idle:
`/history`, `/resume <id>`, `/session <id>`, `/tokens`, and
`/debug log [session-id]`. No shorter aliases are part of the public contract.
Existing `quit`/`exit` and `clear` behavior remains unchanged.

Command suggestions include argument hints for commands that need arguments:

- `history`: `list recent sessions`
- `resume <id>`: `load a saved session`
- `session <id>`: `show session summary`
- `tokens`: `show token totals`
- `debug log [session-id]`: `show recent session log`

`resume <id>` is mutating and is rejected while an agent turn is running.
`history`, `session <id>`, `tokens`, and `debug log [session-id]` are
non-mutating readers and may run while idle or after an error. If a non-mutating
command fails, the current prompt draft is restored.

### CLI Commands

Add subcommands without changing the default TUI startup:

- `thndrs sessions list`
- `thndrs sessions show <id>`
- `thndrs sessions resume <id>`
- `thndrs sessions inspect <id> --format json`
- `thndrs sessions export <id> --format jsonl`
- `thndrs debug tail --lines <n>`
- `thndrs debug session-log <id> --lines <n>`

All global flags that affect workspace discovery remain global flags and appear
before the subcommand, for example `thndrs --cwd <path> sessions list`.

The final subcommand tree is:

```text
thndrs [global-options]
thndrs [global-options] sessions list
thndrs [global-options] sessions show <id>
thndrs [global-options] sessions resume <id>
thndrs [global-options] sessions inspect <id> --format json
thndrs [global-options] sessions export <id> --format jsonl
thndrs [global-options] debug tail --lines <n>
thndrs [global-options] debug session-log <id> --lines <n>
```

`--format` is required for `inspect` and `export` until another format exists.
`--lines <n>` defaults to `200` and is capped at `2000`. `resume` launches the
TUI with the selected transcript loaded and appends new records to the selected
session file after taking the writer lock. `inspect` prints a structured JSON
object. `export` prints renderer-independent JSONL records in stable sequence
order.

### Session Identity

Session IDs remain file stems such as `session-YYYYMMDD-HHMMSS`. Commands may
accept exact ids and unambiguous prefixes. Ambiguous prefixes must fail with a
list of matching ids.

Matching rules:

- Exact id match wins over prefix matches.
- Prefix matching is case-sensitive.
- Prefixes are matched only against file stems, never titles or paths.
- Only files named `session-*.jsonl` in the resolved sessions directory
  participate in lookup.
- No match fails with `session not found: <query>`.
- More than one prefix match fails with `ambiguous session id: <query>` and
  prints matching ids newest-first.
- The implementation should expose this as a small `SessionIdQuery` helper so
  TUI and CLI commands share behavior.

### Inspect And Export

Inspect/export output includes:

- session metadata;
- title and latest rename;
- transcript entries;
- tool started and finished records;
- file-write audit metadata;
- shell execution metadata;
- provider/model/web-search metadata;
- token usage totals;
- loaded `AGENTS.md` metadata;
- activated skill metadata;
- config metadata as `null` until `003_configuration` exposes an effective
  config projection, then as the redacted effective config object.

Inspect/export output must not include provider secrets, raw `.env` contents, or
uncapped raw provider payloads.

`inspect --format json` prints one JSON object:

```json
{
  "schema_version": 1,
  "session": {
    "id": "session-YYYYMMDD-HHMMSS",
    "file": { "raw": "/abs/.thndrs/sessions/session-YYYYMMDD-HHMMSS.jsonl", "display": ".thndrs/sessions/session-YYYYMMDD-HHMMSS.jsonl", "kind": "workspace_relative" },
    "title": "latest title",
    "cwd": { "raw": "/abs/workspace", "display": ".", "kind": "workspace_relative" },
    "started_at": "ISO-8601",
    "updated_at": "ISO-8601",
    "provider": "umans",
    "model": "umans-coder",
    "websearch": "auto",
    "app_version": "0.1.0"
  },
  "counts": {
    "user_messages": 0,
    "assistant_messages": 0,
    "reasoning_messages": 0,
    "tool_calls": 0,
    "file_writes": 0,
    "shell_execs": 0,
    "failures": 0,
    "cancellations": 0,
    "queued_inputs": 0,
    "corrupt_lines": 0
  },
  "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 },
  "transcript": [],
  "tools": [],
  "file_writes": [],
  "shell_execs": [],
  "context": { "sources": [] },
  "skills": [],
  "config": null,
  "warnings": []
}
```

Required `transcript` entry variants:

- `user`: `seq`, `time`, `turn_id`, `text`
- `assistant`: `seq`, `time`, `turn_id`, `text`
- `reasoning`: `seq`, `time`, `turn_id`, `text`
- `tool`: `seq`, `time`, `turn_id`, `call_id`, `name`, `arguments`, `status`,
  `output`
- `status`: `seq`, `time`, `source`, `text`
- `error`: `seq`, `time`, `turn_id`, `text`

`tools` pairs `tool_started` and `tool_finished` records by `call_id` and
`turn_id`. If a start or finish record is missing, the object is still emitted
with `started_at` or `finished_at` set to `null` and a warning is added.

`export --format jsonl` prints one JSON object per readable session record,
sorted by `seq` ascending:

```json
{"schema_version":1,"session_id":"session-YYYYMMDD-HHMMSS","seq":0,"time":"ISO-8601","record_type":"session_meta","record":{}}
```

Every exported line has exactly these top-level fields:

- `schema_version`: export schema version, currently `1`.
- `session_id`: selected session id.
- `seq`: session sequence number.
- `time`: record timestamp.
- `record_type`: renderer-independent record type.
- `record`: redacted record payload.

`record_type` values mirror the durable session record tags:
`session_meta`, `context`, `user`, `assistant_finished`,
`reasoning_finished`, `usage`, `tool_started`, `tool_finished`,
`cancelled`, `failed`, `session_renamed`, `file_write`, `shell_exec`,
`skill_activated`, and `queued_input`. Corrupt lines are skipped and reported
only through `inspect.counts.corrupt_lines` or a CLI warning on stderr; they are
not exported as synthetic records.

### Path Display Policy

Human-facing TUI and CLI output displays paths relative to the resolved
workspace when possible, then relative to `$HOME` as `~/...`, then absolute.
The current workspace itself displays as `.`. Relative paths already stored in
records remain relative if they cannot be resolved against the workspace.

Machine-readable inspect/export uses a path object anywhere a path appears:

```json
{ "raw": "/absolute/or/stored/path", "display": "src/lib.rs", "kind": "workspace_relative" }
```

`kind` is one of `workspace_relative`, `home_relative`, `absolute`, `relative`,
or `unknown`. `raw` preserves the stored or resolved value needed for audit and
migration. `display` follows the human path policy. Session log paths and
session file paths are shown only in CLI output or verbose TUI rows.

### Redaction

Debug log readers, inspect, and export all apply the same deterministic
redaction pass to text fields before display or serialization. Existing
already-redacted session records remain unchanged.

Redaction rules:

- Replace `sk-` style API keys with `sk-[REDACTED]`.
- Replace bearer credentials with `Bearer [REDACTED]`.
- Replace assignment values for secret-shaped keys with `[REDACTED]`.
- Secret-shaped keys include `password`, `passwd`, `api_key`, `apikey`,
  `access_token`, `refresh_token`, `id_token`, `secret`, `token`,
  `authorization`, and `x-api-key`, case-insensitive.
- Redact query parameters with secret-shaped names in logged URLs.
- Never read or print raw `.env` contents; if a log line contains dotenv-shaped
  content, redact it by the same key rules.
- Preserve line count and non-secret context so logs remain useful.

### Resume Display

Resume loads readable records in sequence order and rebuilds transcript rows
through the renderer-independent projection.

Prior unfinished records are displayed as settled informational rows:

- `tool_started` without a matching `tool_finished`: `tool interrupted:
  <name> (#<call_id>)`.
- `tool_finished` without a matching `tool_started`: display the finished tool
  with `#<call_id>` as the name and empty arguments.
- `queued_input`: `queued <kind> input was not replayed`.
- Missing final assistant text after a user turn is represented only by the
  existing `cancelled` or `failed` record if present; otherwise no synthetic
  assistant row is created.
- Running state from a prior process is never restored as live.

## Implementation Shape

### Session Index

Do not introduce a required database. Build a small derived index by scanning
`.jsonl` files and reading summaries through `SessionReader`.

No cache is implemented in this feature. If scanning becomes expensive in a
separate future feature, that feature may add a rebuildable cache under
`.thndrs/cache/`; it is not part of this contract.

### Replay

Replay uses `SessionRecord::to_entry()` plus additional metadata projection
where needed. Replay must tolerate corrupt or partial lines the same way
`SessionReader::read_records` does.

Resume must not replay unfinished live state as running. Running tool records
from a prior process settle as informational status rows.

### Logs

Runtime logs stay text files under `.thndrs/logs`. Debug readers should:

- cap lines;
- tolerate missing files;
- redact secret-looking values before display;
- show file path metadata only in verbose mode or CLI output.

## Decisions

- Session storage remains append-only JSONL. The old SQLite session manager is
  not the storage model for `thndrs`.
- Session lookup and listing only consider files named `session-*.jsonl`.
  Other `.jsonl` files in the sessions directory are ignored.
- `resume <id>` continues the selected session by appending new records after
  the latest readable sequence number. It never rewrites prior records.
- Resume takes an exclusive writer lock before appending. If the session cannot
  be locked, the command fails with a clear message instead of forking hidden
  state.
- Session summaries and indexes are derived from JSONL records and can be
  rebuilt.
- Inspect/export uses renderer-independent session records, not terminal row
  formatting.
- Inspect emits `"config": null` until `003_configuration` provides the
  effective-config projection. Export does not synthesize config records.
- P6 documentation updates happen after the commands exist, but their content
  is already defined by this plan: commands, prefix lookup, inspect/export
  contents and omissions, log readers, and resume behavior.
- Context and memory operations live in `001_context_control`.
- The old workspace split and GUI are outside the implementation path for this
  repository.

## Open Questions

None for P0 through P6.

## Dependencies

- Existing `src/session/mod.rs` JSONL reader/writer.
- Existing runtime tracing paths in `src/lib.rs`.
- Effective config metadata from `003_configuration`; absent metadata is
  represented as `null`.
- UI command-surface polish from `004_ui` is not required for this feature.
  Command handlers and suggestions are implemented directly here.

## Verification

- Unit tests for session id matching.
- Unit tests for listing summaries newest-first.
- Unit tests for corrupt-line tolerance.
- Snapshot or JSON tests for inspect/export.
- TUI command handling tests for `history`, `resume`, `tokens`, and debug log.
- CLI parser tests for all subcommands.
