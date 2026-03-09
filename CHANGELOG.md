# Changelog

## Unreleased

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
