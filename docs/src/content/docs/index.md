---
title: "THNDRS AI Agent"
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
- Umans.ai & OpenCode Go support
- Choose automatic web search, provider-native search, Exa-backed search, or no
  provider-side web search.
- Session history
- Internal inspection for the agent

## Coming Soon

- Headless inspect and export commands for reviewing sessions
- Read-only code-intelligence (LSP-based) tools such as document symbols,
  workspace symbols, go to definition, references, hover, and implementations
  when a language server is available.
- Granular context control
- File (markdown) backed memory
- ChatGPT Codex provider support through `chatgpt-codex/` models and ChatGPT
  subscription auth.
- Better config/session controls
