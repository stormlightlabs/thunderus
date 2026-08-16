---
title: "Security and Permissions"
---

`thndrs` does not treat the TUI as a security boundary. Built-in file tools
operate inside the selected workspace with deterministic limits and audit
records. Process execution needs an OS sandbox, container, or VM to receive
less authority than the current user.

## Tool Boundary

The model can act only through the tools exposed by `thndrs`. File tools reject
paths that escape the workspace root. Search, reads, URL fetching, shell output,
and transcript rendering are bounded by timeouts, byte caps, result caps, and
line truncation.

`thndrs` reports filesystem authority, network authority, and isolation
separately. A boundary report with `isolation=none` means the process inherits
host authority. `isolation=external-unverified` means an ACP client or remote
service owns the execution environment; `thndrs` cannot verify its isolation.
The labels `read-only` and `workspace-write` describe filesystem policies for a
sandbox backend. They do not claim OS enforcement until a backend is active.

`AGENTS.md` files are guidance, not permissions. They can steer behavior, but
they cannot grant extra filesystem access, change tools, or disable safety
limits.

## Shell Commands

`run_shell` executes an argv array with `std::process::Command`. It is not a
raw shell string tool. The command inherits the filesystem and network
permissions of the `thndrs` process. Its boundary report says `isolation=none`.

If a task needs real isolation, run `thndrs` inside a container, VM, or
OS-level sandbox with the filesystem and credentials you are willing to expose.

## MCP Servers

[MCP](/docs/usage/mcp) servers are external tools configured by the user. Stdio
servers run as local child processes with the same filesystem and network
permissions as `thndrs`; their boundary report says `isolation=none`.

Streamable HTTP servers receive requests at the configured URL with the configured
headers. The remote server's filesystem authority and isolation are external to
`thndrs` and reported as unverified.

Project MCP configuration is inactive until the user trusts the workspace and
the file's exact hash. Editing the file invalidates that decision. Global MCP
configuration is active without a project trust step.

Agent-initiated MCP tool calls use the shared permission path. A trust decision
controls whether project servers may start; it does not reduce the operating
system permissions available to a server.

MCP tools cannot replace built-in tool names or change prompt identity. They
are namespaced as `mcp__{server}__{tool}` and use the shared timeout, output
cap, redaction, and session audit path. Those limits bound what `thndrs`
records and shows but do not sandbox what the MCP server itself can do.

## ACP Agents

[ACP](/docs/usage/acp) agents are external local child processes configured by the
user. A configured agent owns its model loop, but `thndrs` still owns the
workspace boundary, TUI permission prompts, cancellation, and local session
records.

ACP filesystem callbacks are workspace-contained. When `thndrs` runs an ACP
terminal callback, the local child inherits the filesystem and network
permissions of `thndrs`; its boundary report says `isolation=none`. When the
ACP client runs a terminal command for the `thndrs` server, that client owns
filesystem, network, and isolation policy. `thndrs` reports that boundary as
external and unverified. In both directions, the requested cwd is kept inside
the workspace, output is capped and redacted, and lifecycle metadata is
recorded in the session log.

## Writes

Write-capable tools are workspace-contained and transcripted. Each create,
replace, and patch write builds the complete new content in a temporary file in
the target directory, flushes and synchronizes it, closes it, and only then
installs it.

A failed write therefore leaves the previous target bytes intact and cleans
up its temporary file.

Creates use a no-clobber install: if another writer creates the target after
validation, the create fails instead of overwriting it.

Replacements preserve the existing target permissions on platforms that expose them.
Replacement installs are atomic on Unix-like platforms. On platforms whose filesystem
does not support replacing an existing path with `rename` (including Windows), the
operation fails without replacing the target.

Session records store write metadata such as path, operation, hashes, and byte counts
but do not store full file contents.

## Secrets

Command output redaction is best effort.

`thndrs` redacts common token patterns in displayed and recorded shell output,
but it cannot guarantee every secret is detected.

Avoid running commands that print credentials.
