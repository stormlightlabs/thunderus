# Tools

`thndrs` exposes typed, bounded tools to the model. The model does not receive
raw shell-string access.

## File Discovery

`find_files` discovers paths inside the selected workspace. It is backed by
`fd` and keeps hidden or ignored files out of the default search.

### Searchable Files

`list_searchable_files` lists files that are reasonable candidates for content
search. It uses `rg --files` or `fd --type file`.

### File Range Reads

`read_file_range` reads a bounded line range from a file using Rust-native file
I/O. Paths must stay inside the selected workspace.

## Text Search

`search_text` searches file contents with `rg --json` and returns structured
match records. `rg` exit code `1` is treated as no matches, not as a tool
failure.

## Safety Limits

Tool wrappers enforce workspace containment, timeouts, output caps, max result
counts, max line lengths, and transcript truncation. Hidden files, ignored
files, symlink following, and unrestricted searches are not default behavior.

## Transcript Rendering

Tool calls render as structured transcript entries with the tool name, status,
arguments summary, output summary, and truncation state.

## File Writes

The write tools are `create_file`, `replace_range`, and `write_patch`.
`create_file` fails if the file already exists. `replace_range` edits one
unique exact string occurrence. `write_patch` is the unified create, replace,
or edit entry point. All write paths must stay inside the workspace root.

## Shell

`run_shell` runs a program plus argv list in the workspace. Prefer narrower
tools for file discovery, search, reads, edits, and URL reads. Use shell for
builds, tests, formatters, and commands that do not fit a narrower tool.

The shell tool is not a sandbox. Commands run as the local `thndrs` process.
If shell syntax is required, the model must invoke an explicit shell program in
argv, such as `sh` with `-c`; `thndrs` does not add a separate command-string
tool.

## Running Input

While the agent is running, typed input can be queued instead of ignored.
`Ctrl+T` toggles the target between steering and follow-up. Steering is sent
before the next model request in the active run. Follow-up is submitted as a
new user turn after the active run finishes. `Esc` cancels the active run and
clears pending steering for that run; queued follow-ups remain until submitted
or cleared with `/clear`.
