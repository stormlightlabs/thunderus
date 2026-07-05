---
title: "Tool Reference"
---

The tool catalog is sent to the provider as structured tool schemas. Tool names
and input fields are part of the user-visible contract, but output may be capped
or summarized for the transcript.

The public catalog is derived from the built-in tool registry in `src/tools/`.
Schema descriptions and examples below should match the tool modules that parse
and execute them.

## `find_files`

Locate files by name or glob under the workspace root.

Inputs:

- `pattern`: file name or glob pattern.
- `glob`: optional additional glob filter.
- `extensions`: optional extension filter.
- `max_depth`: optional traversal depth.
- `include_hidden`: include hidden files.
- `follow_symlinks`: follow symlinks.

Hidden files and symlinks are off by default. Results are capped.

## `list_searchable_files`

Enumerate files that are reasonable candidates for content search.

Inputs:

- `glob`: optional glob filter.
- `include_hidden`: include hidden files.

Prefer `find_files` when the target name is known and `search_text` when the
target content is known.

## `search_text`

Search file contents with a regex and return `file:line:text` matches.

Inputs:

- `pattern`: regex pattern.
- `glob`: optional glob filter.
- `extensions`: optional extension filter.
- `context_lines`: optional surrounding line count.
- `include_hidden`: include hidden files.

`rg` exit code `1` is treated as no matches. Matches and line length are capped.

## `read_file_range`

Read a 1-indexed line range from a file.

Inputs:

- `path`: path relative to the workspace root.
- `start_line`: first line to read.
- `end_line`: optional final line.

Prefer targeted ranges over reading large files.

## `web_search`

Search the web for current information when the workspace does not contain the
answer.

Inputs:

- `query`: search query.
- `max_results`: optional result cap.

Search behavior depends on the selected web-search mode.

## `read_url`

Fetch a public `http` or `https` URL and extract readable text.

Inputs:

- `url`: public URL to fetch.

Private-network targets, unsupported schemes, binary content, oversized
responses, and redirect chains beyond the configured limit are rejected.

## `create_file`

Create a new file with full content.

Inputs:

- `path`: path relative to the workspace root.
- `content`: full file content.

The tool fails if the file already exists and creates parent directories when
needed.

## `replace_range`

Replace one unique exact string occurrence in an existing file.

Inputs:

- `path`: path relative to the workspace root.
- `old_string`: exact string to find.
- `new_string`: replacement string.

The edit fails if `old_string` appears zero or multiple times. Failed edits
leave the file unchanged.

## `write_patch`

Unified write entry point for create, replace, and exact edit operations.

Inputs:

- `op`: `create`, `replace`, or `edit`.
- `path`: path relative to the workspace root.
- `content`: full content for `create` and `replace`.
- `old_string`: exact string for `edit`.
- `new_string`: replacement string for `edit`.

Use `edit` for small exact replacements, `create` for new files, and `replace`
only when overwriting the full file is intentional.

## `run_shell`

Run a local program in the workspace and capture stdout, stderr, status, and
timing.

Inputs:

- `program`: program name.
- `args`: argv after the program.
- `cwd`: optional working directory relative to the workspace root.
- `timeout_secs`: optional timeout.
- `background`: run as a long-lived background process.

Commands run as the local `thndrs` process, not in a sandbox. Output is capped,
truncated, and redacted where deterministic redaction is possible.

## Side-Effect Records

Every tool call has a `tool_started` and `tool_finished` session record. Write
tools also append `file_write` audit metadata with operation, path, hashes, byte
counts, and status. `run_shell` also appends `shell_exec` metadata with command,
working directory, process kind, exit status, elapsed time, and cancellation or
background state. Full file contents and uncapped stdout/stderr are not stored
in those side-effect records.
