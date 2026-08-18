---
title: "Tools"
---

`thndrs` exposes typed tools to the model. The model does not receive
raw shell-string access.

The visible tool list comes from the built-in registry. Each registered tool has
one stable name, one provider schema, one executor, and one example input used
by tests. Configured MCP tools are appended at runtime after server discovery
and appear with `mcp__{server}__{tool}` names.

Built-in tool modules own their schemas, parsing, execution, and side-effect
metadata. Providers receive the catalog derived from that registry, so a tool
cannot be exposed without an executor or executed without a schema. See the
[Tool Reference](/docs/reference/tools/) for exact input fields.

## File Discovery

`find_files` discovers paths inside the selected workspace. It uses `fd` when
available and falls back to `find`. Both paths keep hidden files out of the
default search. `fd` also respects ignore files.

### Searchable Files

`list_searchable_files` lists files that are reasonable candidates for content
search. It uses `fd --type file`, `rg --files`, or `find`, in that order.

### File Range Reads

`read_file_range` reads a line range from a file using Rust-native file
I/O. Paths must stay inside the selected workspace.

## Text Search

`search_text` searches file contents with `rg --json` and returns structured
match records. `rg` exit code `1` is treated as no matches, not as a tool
failure.

## Safety Limits

Tool wrappers enforce workspace containment, timeouts, output caps, max result
counts, max line lengths, and transcript truncation. Hidden files, ignored
files, symlink following, and unrestricted searches are not default behavior.

`AGENTS.md` files can guide tool choice and project conventions. They cannot
grant filesystem access, change the tool catalog, or disable these limits.

MCP tool calls share the same transcript and output limits, but the MCP server
itself is external. Stdio servers run as local child processes and Streamable
HTTP servers run wherever their configured endpoint points.

## Transcript Rendering

Tool calls render as structured transcript entries with the tool name, status,
arguments summary, output summary, and truncation state.

Session logs also keep side-effect audit records for tools that modify or run
local state. File writes record operation, path, before/after hashes, byte
counts, and status. Shell runs record command, working directory, process kind,
status, exit code, elapsed time, and background/cancellation state. The audit
records avoid storing full file contents or uncapped process output.

## File Writes

The write tools are `create_file`, `replace_range`, and `write_patch`.
`create_file` fails if the file already exists. `replace_range` edits one
unique exact string occurrence. `write_patch` is the unified create, replace,
or edit entry point. Its `patches` array accepts one create/replace operation or
one or more edits for the same file. Batched edits all match the original file,
and the tool validates them before writing. Relative paths resolve from the
workspace root; absolute paths and paths outside it are allowed when the
`thndrs` process can access them. Writes complete and synchronize
same-directory temporary files before installation, so failed writes preserve
the previous target; creates also fail rather than clobbering a target that
appears during validation.

## Shell

`run_shell` takes one `argv` array containing the program and its arguments. It
runs that command in the workspace. Prefer narrower tools for file discovery,
search, reads, edits, and URL reads. Use shell for builds, tests, formatters,
and commands that do not fit a narrower tool.

The shell tool is not a sandbox. Commands run as the local `thndrs` process.
If shell syntax is required, the model must invoke an explicit shell program in
argv, such as `sh` with `-c`; `thndrs` does not add a separate command-string
tool. With `background: true`, the interactive application owns the child and
returns its id without waiting. Use `:bg` to list live children or
`:bg cancel <id>` to cancel one; quitting reaps all owned children. Background lifecycle
events are recorded separately from the initial tool result.

Configured [MCP tools](/docs/usage/mcp/) enter through the external-tool path. Their
provider names are namespaced, and their results use the same output limits,
redaction, and session log path as built-in tools. Those limits constrain what
`thndrs` records and displays; they do not constrain what an MCP server can do.

## Running Input

While the agent is running, typed input can be queued instead of ignored.
`Enter` queues a follow-up, submitted as a new user turn after the active run
finishes. `Ctrl+G` queues steering, sent before the next model request in the
active run. `Cmd+Enter` on macOS or `Ctrl+Enter` elsewhere also queues steering
when the terminal forwards that chord.

Queued input is written to the session log as audit metadata, but resume does
not rebuild pending queues after a crash. If that audit append fails, the TUI
keeps the queued input buffer and shows an error in the transcript.

Slash commands are restricted while a run is active. `/help`, `/bg`, `/bg cancel <id>`, `/quit`,
and `/exit` can run immediately; commands that mutate idle-only UI state, such
as `/clear`, `/model`, and `/skills`, are rejected until the run finishes. To
queue a follow-up that intentionally starts with `/`, prefix it with `//`; for
example `//clear after this run` queues `/clear after this run` as text.

`Esc` cancels the active run and clears pending steering for that run. Queued
follow-ups remain until they are submitted after the active run, cancelled by
quitting the app, or cleared later with `/clear` while idle.
