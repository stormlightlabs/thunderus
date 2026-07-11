---
title: "Session Format"
---

Sessions are append-only JSONL files under `.thndrs/sessions/`. Each line is a
single record with `schema_version`, `seq`, `time`, and `type`.

The current schema version is `1`. `seq` is monotonic within a session. Records
are appended and not rewritten.

## Record Types

- `session_meta`: session id, cwd, title, provider, model, websearch mode, and
  app version.
- `context`: loaded `AGENTS.md` metadata: path, scope, content hash,
  truncation state, and byte count.
- `context_ledger`, `context_pin`, `context_drop`, and `context_recovery`:
  content-free context working-set and decision evidence.
- `memory_write` and `memory_delete`: content-free memory mutation audit data.
- `compaction`: a redacted summary plus covered ranges, hashes, review state,
  and recovery handles.
- `user`: user prompt text and turn id.
- `assistant_finished`: final replayable assistant text.
- `reasoning_finished`: final replayable reasoning text.
- `usage`: provider token usage increment.
- `tool_started`: tool call id, tool name, and arguments.
- `tool_finished`: tool call id, status, and capped/redacted output.
- `file_write`: file write audit metadata.
- `shell_exec`: shell command lifecycle metadata.
- `mcp_config_changed`: MCP configuration file hashes and diagnostics.
- `skill_activated`: opened skill metadata and reference hashes.
- `queued_input`: queued steering or follow-up input audit data.
- `acp_session`: external ACP session metadata.
- `acp_permission_request`: ACP permission prompt metadata.
- `acp_permission_outcome`: ACP permission outcome metadata.
- `cancelled`: cancelled turn and reason.
- `failed`: failed turn and error message.
- `session_renamed`: updated session title.

## File Write Records

`file_write` stores operation type, target path, before/after content hashes,
byte counts, and status. It does not store full file content.

## Shell Records

`shell_exec` stores the command, working directory, process status, exit code,
elapsed time, and whether the process was one-shot or background. Stdout and
stderr are represented through the corresponding capped `tool_finished` output.

## ACP Records

`acp_session` stores the configured ACP agent name, opaque external ACP session
id, redacted command display, selected protocol version, and optional agent info.
The local `thndrs` session id and the external ACP session id are stored as
separate fields.

ACP permission records store the prompt title, option metadata, and selected or
cancelled outcome. They do not store credentials or raw protocol stdio lines.

## Prompt Metadata

Session context records preserve what prompt context was loaded without storing
full `AGENTS.md` contents in metadata. Raw provider request and response payloads
are not persisted by default.

Prompt metadata also records compact self-knowledge inputs. This supports prompt
inspection and replay audits without persisting full prompt text, project instruction
text, or provider-private state.

## Inspection and Export

`thndrs sessions inspect <id> --format json` projects valid records without
depending on the terminal renderer. `export --format jsonl` writes the same
records one per line in sequence order. Both views redact secret-looking values
and skip malformed lines, while preserving every valid record after a damaged
line. They never replay a stored action.
