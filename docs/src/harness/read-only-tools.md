# Read-Only Tools

`thndrs` exposes typed, bounded tools to the model. The model does not receive
raw shell access.

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
