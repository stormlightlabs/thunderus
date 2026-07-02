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
- `user`: user prompt text and turn id.
- `assistant_finished`: final replayable assistant text.
- `reasoning_finished`: final replayable reasoning text.
- `usage`: provider token usage increment.
- `tool_started`: tool call id, tool name, and arguments.
- `tool_finished`: tool call id, status, and capped/redacted output.
- `file_write`: file write audit metadata.
- `shell_exec`: shell command lifecycle metadata.
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

## Prompt Metadata

Session context records preserve what prompt context was loaded without storing
full `AGENTS.md` contents in metadata. Raw provider request and response payloads
are not persisted by default.

Prompt metadata also records compact self-knowledge inputs. This supports prompt
inspection and replay audits without persisting full prompt text, project instruction
text, or provider-private state.
