---
title: "Tool Boundary"
---

`thndrs` gives the model typed tools rather than unrestricted direct access to
the machine. The boundary is practical and auditable, not a security sandbox.

## Registry Source Of Truth

Built-in tools are registered in one static Rust registry. Each tool module owns
its provider-visible schema, argument parsing, execution, output mapping, and
side-effect metadata. Provider request bodies, prompt snapshots, and tests derive
their tool catalog from that registry, so a tool cannot be exposed without an
executor or executed without a schema.

## Narrow Tools First

The prompt and tool descriptions steer the model toward the narrowest tool that
fits the task:

- `find_files` for path discovery.
- `search_text` for content search.
- `read_file_range` for file reads.
- `create_file`, `replace_range`, and `write_patch` for file writes.
- `read_url` for public web pages.
- `run_shell` for builds, tests, formatters, and project commands that do not
  fit a narrower tool.

## Workspace Containment

Filesystem tools resolve paths relative to the selected workspace root and
reject attempts to escape that root. Hidden files, ignored files, symlinks, and
broader traversal are opt-in only where a tool supports them.

## Execution Limits

Tool execution is bounded by timeouts, result limits, byte caps, and line-length
caps. Truncated output is marked so both the UI and model-visible transcript can
show that the result was incomplete.

## Shell Boundary

`run_shell` executes a program plus argv list with `std::process::Command`. It is
not a raw shell-string tool. If shell syntax is needed, the model must call an
explicit shell program such as `sh` with `-c`.

Shell commands run with the permissions of the local `thndrs` process. Command
visibility, cancellation, output caps, redaction, and transcript records help
with review, but they are not isolation. For real isolation, run `thndrs` inside
an OS/container/VM or policy sandbox.

## Side-Effect Audit

Tool calls always produce a `tool_finished` session record with capped,
redacted output. File-write tools also return structured write metadata:
operation, path, before/after hashes, byte counts, and status. `run_shell`
returns structured process metadata: command, working directory, process kind,
status, exit code, elapsed time, and cancellation/background state. The session
layer records those side effects without special-casing individual tools.

## External Tools

MCP tools are planned for the external-tool path instead of the built-in path.
They will use namespaced names such as `mcp__server__tool`, share the same
execution result shape, and keep their own discovery/configuration lifecycle.
Keeping MCP external prevents server-provided tools from rewriting built-in
schemas, prompt identity, or local safety rules.

## Repository Guidance

`AGENTS.md` files are guidance. They can explain project conventions and useful
commands, but they cannot grant permissions, change tools, disable safety
limits, reveal secrets, or override direct user and harness instructions.
