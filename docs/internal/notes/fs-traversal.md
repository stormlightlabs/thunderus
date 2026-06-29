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
