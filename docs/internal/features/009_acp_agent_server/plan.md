---
title: ACP Packaging And Expansion
status: Draft
captured: 2026-07-10
---

## Objective

Validate the baseline `thndrs acp serve` mode with a real editor client and
prepare registry metadata. Stdio remains the only supported transport.

## Baseline Capability

The baseline owns and must preserve the existing server behavior:

- ACP stdio version negotiation, session creation/load/resume/close, prompt
  turns, cancellation, and semantic `session/update` streaming;
- safe tool-call reporting, redacted/capped output, and client permission for
  writes and shell execution;
- configuration/session metadata, local JSONL audit, client filesystem and
  terminal capability handling, MCP configuration, and supported rich content;
- fake-client regression coverage with protocol-clean stdout.

## Executable Design

`thndrs acp serve` is a mode of the primary executable. It reuses the same
provider, tool, configuration, and session runtime as the CLI/TUI while ACP
transport, request handling, and client permission RPC remain in the server
adapter.

The public executable is:

```text
thndrs [--cwd <path>] [--model <model>] [--websearch <mode>] [--session-dir <path>] acp serve
```

stdout contains only ACP JSON-RPC. Diagnostics use stderr or configured
tracing sinks.

## Success Criteria

- The server mode retains all baseline ACP fixtures and local audit
  behavior.
- A real editor/client path demonstrates initialization, prompt streaming,
  cancellation, permissions, and sessions without stdout pollution.
- Registry material names the executable command and its actual capabilities;
  checks run without publishing.
- Non-stdio work begins only for a documented concrete deployment that cannot
  use a local stdio executable.

## Boundaries

Always:

- treat ACP session ids as opaque;
- preserve local tool containment, output caps, redaction, and permission
  policy;
- test every compatibility fix with the fake client.

Ask first:

- a `thndrs-acp-client` library;
- persistent approvals, new transports, or unstable ACP SDK features.

Never:

- write diagnostics to stdout;
- auto-approve side effects or store raw secrets;
- let editor-driven mode bypass local safety rules.

## Deferred

- A reusable `thndrs-acp-client` library only after a demonstrated separate
  client boundary.
- Streamable HTTP, WebSocket, or custom transport only for a specific target
  that stdio cannot serve.

## Verification

```text
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test -p thndrs --test acp_server_smoke
cargo run -p thndrs -- acp serve
```

The detailed implementation frontier is in [tasks.md](tasks.md).
