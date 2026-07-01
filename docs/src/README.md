# Introduction

## What thndrs Is

`thndrs` is an agentic coding harness. It provides a terminal UI for
chatting with a coding model, showing reasoning and tool activity, loading
project guidance, and safely inspecting a repository.

## Implemented

- Direct terminal renderer with transcript entries emitted into native
  terminal scrollback and only the live prompt/status region redrawn.
- Clap-based CLI with model, workspace, web-search, tick-rate, terminal, and
  prompt-inspection options.
- Umans provider support for `umans-coder` and `umans-glm-5.2`.
- Streaming assistant, reasoning, tool, status, and error transcript entries.
- Root `AGENTS.md` loading with visible context metadata and truncation handling.
- Read-only repository tools for file discovery, text search, and file-range
  reads.
- Native, Exa, and disabled web-search modes.
- Prompt assembly via a structured prompt bundle before provider lowering.
- Unit tests and renderer row-model/backend snapshot tests.

## Coming Soon

- Session persistence.
- Safe file-edit operations.
- Config file support.
- Non-TUI inspect and export commands.
