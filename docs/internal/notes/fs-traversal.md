---
Title: Filesystem Traversal and Text-Processing Tools for Agents
Author: Andrew Gallant, sharkdp/fd contributors, GNU sed/gawk maintainers, BSD/macOS manpages
Date: 2026-06-28
Captured: 2026-06-28
Tags: [agent-tools, filesystem, ripgrep, fd, sed, awk, safety, rust]
Sources: >
  - https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md
  - https://github.com/sharkdp/fd#readme
  - https://www.gnu.org/software/sed/manual/sed.html
  - https://www.gnu.org/software/gawk/manual/gawk.html
  - Local help/manual output: `rg --help`, `fd --help`, `man sed`, `man awk`
---

## Summary

`thndrs` should integrate `fd` and `ripgrep` as structured read-only discovery
tools, while treating `sed` and `awk` as constrained text-inspection helpers
rather than arbitrary model-controlled shell programs.

## Key Ideas

- **Prefer purpose-built wrappers over raw shell:** Agent tools should expose
  typed inputs like `pattern`, `path`, `glob`, `max_results`, and `context`
  instead of accepting arbitrary command strings.
- **Use `fd` for path discovery:** `fd` has repo-friendly defaults: it skips
  hidden files and respects ignore rules by default, while offering filters for
  type, extension, depth, size, path matching, and result limits.
- **Use `rg` for content search:** `ripgrep` respects ignore rules, skips hidden
  and binary files by default, supports globs and file types, and can emit JSON
  Lines for machine parsing.
- **Keep `sed` output-only in the harness:** `sed` is valuable for line-range
  printing and substitution previews, but `-i`, `w`, and GNU `e` can mutate files
  or execute commands. Prefer Rust-native line slicing for common reads.
- **Keep `awk` templated or human-approved:** `awk` is excellent for field
  extraction and summaries, but programs can redirect output, open pipes, and
  call `system()`. Do not run model-authored arbitrary `awk` by default.
- **Read-only tooling still needs caps:** Traversal depth, max results, max bytes,
  stdout/stderr caps, timeout, and project-root containment should be enforced
  by `thndrs`, not left to model behavior.

## Claims & Evidence

| Claim                                                             | Support                                                                                                                                                                                                                                                                                                      | Caveat / Confidence                                                              |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| `rg` is the right default content-search primitive.               | ripgrep's guide documents recursive search, automatic filtering, `.gitignore`/`.ignore`/`.rgignore` support, hidden-file skipping, binary-file skipping, globs, file types, and `--files`. Local `rg --help` confirms JSON Lines output via `--json`.                                                        | High. Need handle exit code `1` as "no matches," not a tool failure.             |
| `fd` is the right default path-discovery primitive.               | fd's README describes it as a filesystem entry finder with regex/glob patterns, parallel traversal, hidden/ignored defaults, type/extension filters, and command execution options. Local `fd --help` confirms `--max-depth`, `--max-results`, `--type`, `--extension`, `--print0`, and `--one-file-system`. | High. Do not expose `-x`/`-X` execution to the model.                            |
| `sed` should not be a write tool.                                 | GNU sed and BSD/macOS sed support in-place editing, and sed scripts can write files. macOS manpage warns about corruption/partial content risk with in-place editing without backups.                                                                                                                        | High. Also portability differs between GNU and BSD sed.                          |
| `awk` should be constrained because it is a programming language. | gawk manual describes pattern-action programs, fields, built-ins, redirection, pipes, and `system()`. Local `man awk` confirms fields, `print`/`printf`, redirection, pipes, and `system()`.                                                                                                                 | High. Useful for summaries, risky as arbitrary model code.                       |
| Tool output should be structured for the UI/model.                | Existing roadmap uses typed `AgentEvent` and transcript entries; `rg --json` gives structured search events, and `fd` output can be parsed line-by-line or NUL-delimited.                                                                                                                                    | High. `fd` does not provide JSON, so use NUL or newline plus path normalization. |

## Important Terms

| Term                     | Meaning                                                                                                                                 |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| `fd`                     | Fast filesystem entry finder; best used for path discovery and filtered file listing.                                                   |
| `rg` / ripgrep           | Fast recursive content search; best used for grep-like search and candidate file discovery.                                             |
| `sed`                    | Stream editor; useful for line-oriented output transforms, but capable of in-place edits and file writes.                               |
| `awk`                    | Pattern-action language for scanning records and fields; useful for summaries, but capable of command execution and output redirection. |
| Tool wrapper             | Rust function that maps typed input to a fixed command invocation and structured output.                                                |
| Project root containment | Rejecting paths that escape the selected workspace root after canonicalization.                                                         |

## Integration Plan

### Tool Set

Add these as read-only alpha tools before any write-capable operations:

- `find_files`: backed by `fd`.
- `search_text`: backed by `rg --json`.
- `list_searchable_files`: backed by `rg --files` or `fd --type file`.
- `read_file_range`: implement in Rust first; optionally allow `sed -n` only as
  a compatibility fallback.
- `summarize_text`: optional canned `awk` templates for safe, bounded summaries.

Do not expose:

- `fd --exec` or `fd --exec-batch`.
- `rg --pre`, `--pre-glob`, or compressed search initially.
- `sed -i`, `sed -I`, `sed w`, GNU `sed e`, or arbitrary sed scripts.
- arbitrary `awk` programs, `awk system()`, redirections, or pipes.

### `fd` Wrapper

Inputs:

- `pattern: Option<String>`
- `root: Option<PathBuf>`
- `kind: file | directory | symlink | any`
- `glob: bool`
- `extensions: Vec<String>`
- `include_hidden: bool`
- `include_ignored: bool`
- `max_depth: Option<u32>`
- `max_results: u32`

Default command shape:

```text
fd --color never --type file --max-results N PATTERN ROOT
```

Rules:

- Default to hidden and ignored files excluded.
- Only enable hidden/ignored files when explicitly requested.
- Prefer relative paths for transcript readability.
- Normalize and reject paths outside the selected root.
- Do not follow symlinks by default.
- Consider `--one-file-system` for large or mounted workspaces.

### `rg` Wrapper

Inputs:

- `pattern: String`
- `paths: Vec<PathBuf>`
- `globs: Vec<String>`
- `file_type: Option<String>`
- `context: u8`
- `case_mode: sensitive | insensitive | smart`
- `max_matches: u32`
- `max_columns: u32`

Default command shape:

```text
rg --json --line-number --column --color never --max-columns N PATTERN PATH...
```

Rules:

- Parse JSON Lines into match records: path, line, column, text, submatches.
- Treat exit code `1` as no matches.
- Keep `--hidden`, `--no-ignore`, `--text`, and `--follow` opt-in.
- Use `--glob`/`--type` for bounded searches instead of searching everything.
- Cap output even when `rg` would produce many matches.
- Avoid `--vimgrep` as the primary integration; `rg --help` recommends JSON for
  editor integrations over `--vimgrep` when possible.

### `sed` Role

Use cases:

- Print a line range for quick context.
- Preview simple substitutions on stdout.
- Show transformed text without touching the file.

Recommendation:

- Implement `read_file_range` in Rust instead of invoking `sed`.
- If `sed` is used, only permit `-n` plus generated address/print scripts.
- Never pass model-authored sed scripts directly.
- Never allow in-place editing from the read-only tool boundary.

Portability note:

- Local `sed` is BSD/macOS style and does not support `--help`.
- GNU sed and BSD sed differ in extensions and `-i` behavior. Keep any required
  sed usage POSIX-ish or avoid it.

### `awk` Role

Use cases:

- Count rows/fields in logs or delimited data.
- Extract a column from a bounded text stream.
- Build simple summaries from command output.

Recommendation:

- Prefer canned templates over arbitrary `awk` programs.
- Feed bounded input through stdin, not project-wide files directly.
- Strip or reject programs containing redirection, pipes, or `system(` if raw
  awk ever becomes a human-approved advanced mode.
- Default to Rust-native parsing when the format is known.

## Agent Tool Design

The model should see high-level tools, not command lines:

```text
find_files(pattern, kind, extensions, max_depth, max_results)
search_text(pattern, paths, globs, context, max_matches)
read_file_range(path, start_line, end_line)
summarize_text(path, mode)
```

Each tool result should include:

- `command`: sanitized display command, not necessarily the exact argv.
- `cwd`: workspace-relative root.
- `exit_status`: numeric status or killed/timeout.
- `truncated`: whether output was capped.
- `results`: structured records.
- `stderr`: capped diagnostic text.

Execution rules:

- Use `std::process::Command` with argv arrays, not shell strings.
- Run from the selected workspace root.
- Clear or tightly control environment variables.
- Enforce timeout and output-byte limits.
- Enforce max result counts in the wrapper even if the child command supports
  its own limit.
- Treat paths as data, not shell syntax.
- Reject absolute paths or `..` escapes after canonicalization unless explicitly
  allowed by a future workspace config.

## Release Mapping

- fake/v0: no shell-backed traversal required. Fake tool entries are enough.
- alpha: add `find_files`, `search_text`, `list_searchable_files`, and
  `read_file_range` as read-only tools.
- alpha later: add Lectito-backed web extraction/search separately from local fs
  traversal.
- v1: document command availability, output caps, path containment, and
  portability assumptions. Add integration tests with fixture directories.

## Claims To Preserve In Specs/TODO

- `fd` and `rg` are alpha read-only tooling, not v1-only polish.
- `sed` and `awk` should not be exposed as arbitrary raw programs in the first
  usable release.
- Safe file edits should remain separate from search/traversal tools.
- Tool output should be structured so the TUI can render search results without
  parsing terminal text.

## Questions for Review

- Should `search_text` use `rg --json` from day one, or a simpler line parser
  first?
- Should `find_files` prefer `fd` or `rg --files` for default file discovery?
- Do we want an advanced human-approved "run awk" mode after v1, or should
  summaries stay template-only?
- What is the default output cap per tool call: lines, bytes, or both?

## Connections

- Related ideas: Pi's explicit small tool set; Herdr's semantic tool/process
  states; Ratatui snapshots for tool-result rendering; alpha read-only tool
  boundary.
- Related sources: [pi](./pi.md), [herdr](./herdr.md), [release](./release.md).
- Contradictions or tensions: developer muscle memory favors raw shell commands,
  but an agent-facing harness needs typed, bounded, auditable operations.
- Useful applications: reliable repo search, file discovery, context gathering,
  and safe transcript rendering before write-capable tools exist.

## Open Questions

- Whether to vendor Rust crates for search/traversal later (`ignore`, `grep`,
  `walkdir`) instead of spawning `fd`/`rg`.
- How to handle Windows environments where `sed`/`awk` may be absent.
- Whether user config should allow hidden/ignored files globally or only per
  tool call.
- How much stderr should be shown in the transcript versus hidden in diagnostics.

## Notable Quotes

> "ripgrep will never modify your files."

## Takeaways

- Make `fd` and `rg` first-class structured read-only tools for alpha.
- Keep `sed` and `awk` constrained to safe, output-only, template-driven use.
- Build safety into the Rust wrappers: no shell, root containment, timeouts,
  output caps, and structured results.
