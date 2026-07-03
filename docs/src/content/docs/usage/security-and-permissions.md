---
title: "Security and Permissions"
---

`thndrs` does not treat the TUI as a security boundary. It runs local tools as
the current user, inside the selected workspace, with deterministic limits and
audit records.

## Tool Boundary

The model can act only through the tools exposed by `thndrs`. File tools reject
paths that escape the workspace root. Search, reads, URL fetching, shell output,
and transcript rendering are bounded by timeouts, byte caps, result caps, and
line truncation.

`AGENTS.md` files are guidance, not permissions. They can steer behavior, but
they cannot grant extra filesystem access, change tools, or disable safety
limits.

## Shell Commands

`run_shell` executes an argv array with `std::process::Command`; it is not a
raw shell string tool. The command runs with the permissions of the `thndrs`
process and is not sandboxed by approval prompts or in-process policy.

If a task needs real isolation, run `thndrs` inside a container, VM, or
OS-level sandbox with the filesystem and credentials you are willing to expose.

## Writes

Write-capable tools are workspace-contained and transcripted. Failed writes
leave the target unchanged where the operation can be made atomic. Session
records store write metadata such as path, operation, hashes, and byte counts;
they do not store full file contents.

## Secrets

Command output redaction is best effort. `thndrs` redacts common token patterns
in displayed and recorded shell output, but it cannot guarantee every secret is
detected. Avoid running commands that print credentials.
