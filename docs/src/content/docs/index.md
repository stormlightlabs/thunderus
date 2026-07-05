---
title: "thndrs"
---

`thndrs` is an agentic coding harness meant to act as an LLM powered pair
programmer. It provides a terminal UI for chatting with an LLM, while it
shares its reasoning and tool activity, loading project guidance, and safely
inspecting a repository.

## Features

- Chat with a coding agent in the terminal while keeping normal terminal
  scrollback, search, and text selection.
- `AGENTS.md` support
- `SKILL.md` support
- [Umans.ai](https://app.umans.ai/offers/code/docs) & [OpenCode Go](https://opencode.ai/docs/go/)
  support
- Choose automatic web search, provider-native search, Exa-backed search, or
  [no provider-side](https://lectito.stormlightlabs.org/docs/) web search.
- Agent Client Protocol support: use external ACP agents from the TUI, or run
  `thndrs-acp-server` so editors and IDEs can drive the `thndrs` harness over
  stdio.
  - See [ACP](/usage/acp/) for agent configuration, editor setup, supported
    capabilities, permission behavior, and troubleshooting.
- Session history
- Internal inspection for the agent

## Coming Soon

- Read-only code-intelligence (LSP-based) tools such as document symbols,
  workspace symbols, go to definition, references, hover, and implementations.
- ChatGPT Codex provider support through `chatgpt-codex/` models and ChatGPT
  subscription auth.
- Granular context control & file (markdown) backed memory
- Better session observability & config/session controls
