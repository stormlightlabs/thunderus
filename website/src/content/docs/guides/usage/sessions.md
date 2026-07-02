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
