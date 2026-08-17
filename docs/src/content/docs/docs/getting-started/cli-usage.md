---
title: "CLI Usage"
---

## Running the App

Running `thndrs` without a subcommand launches the TUI.

```sh
thndrs
```

## Working Directory

Use `--cwd` to select the workspace used for context loading, display, and
read-only tools.

```sh
thndrs --cwd /path/to/repo
```

## Model Selection

A fresh install has no default model. `thndrs` opens required setup, where you
choose a provider and model before submitting a coding prompt. After setup,
`--model` overrides that selection for one run.

```sh
thndrs --model opencode/big-pickle
```

Built-in provider model prefixes include:

- `chatgpt-codex/<model-id>`, for ChatGPT-backed Codex
- `opencode/<model-id>`, for OpenCode Zen
- `opencode-go/<model-id>`, for OpenCode Go

ChatGPT Codex and OpenCode are the supported built-in workflows. Configured
ACP agents remain available as advanced integrations.

## Web searching

Configure an MCP server that provides search when the agent needs to discover
pages. `read_url` can read a known public URL without a search server. See
[Web search and URL reading](/docs/usage/web-search/).

## Prompt Inspection

Use `--print-prompt` to print the assembled system prompt bundle and lowered
provider messages without calling the provider.

```sh
thndrs --print-prompt
```

The output redacts secrets.

## Terminal Options

Use `--tick-rate-ms` to tune UI tick timing. Completed transcript entries stay
in native terminal scrollback; Ratatui redraws only the active prompt and
bounded focused views. Use your terminal's normal scroll, selection, and copy
controls for transcript history.
