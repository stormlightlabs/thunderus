# Changelog

## Unreleased

### 2026-03-09

- Persistent agent memory via per-workspace and global SQLite databases containing `memories`
  and `embeddings` tables with packed float32 BLOBs, cosine similarity search,
  deduplication (>0.95), 90-day decay, conversation/session/log persistence.
- `memory_store` and `memory_recall` tools with implicit recall injected into system prompt
- `thunderus debug memory recall/stats` CLI commands
- TUI: `/history`, `/resume`, `/clear`, `/tokens`, `/model`, `/debug memory`, `/debug log` commands.
- Per-tool argument formatting in `format_tool_arguments` (`chat.rs`), removing generic key/value fallback.
  - `read` tool output: line-numbered display, directory/image placeholders, truncation indicator.
  - `write` tool output: `Wrote {n} bytes -> {path}` success line.
  - `edit` tool output: file path header extracted from diff `+++ b/{path}`.
  - `bash` tool output: truncation indicator and exit code badge.
  - `research` tool output: URL header, wrapped body, truncation indicator.

### 2026-03-08

- Tool calling flow with `read`, `write`, `edit`, `bash`, and `research` tools,
  definitions serialized from `meta/TOOLS.txt`
  - Multi-turn tool loop for OpenAI Completions protocol.
- Tool call UI: name/args/status rows, collapsible output, diff rendering, bash output
  display, and loading state with task progress list.
- File browser with tree walker (`.gitignore`-aware), syntect syntax highlighting, split
  pane layout, breadcrumb path bar, and `@` fuzzy file finder (nucleo).
- Slash command implementation `/`, `/debug chat`, and `/debug files` views.

### 2026-03-02

- Provider-backed multi-turn conversation with system prompt/response-format/tool
  injection, live SSE delta streaming into the chat UI, parsed Intent/Actions/Result/Next
  rendering, and `--stream` debug support for Moonshot and Zhipu.
- Runnable `thunderus` TUI, provider debug CLI, config loading from `~/.thunderus/config.toml`,
  and OpenAI-compatible Moonshot integration with request/response validation.
