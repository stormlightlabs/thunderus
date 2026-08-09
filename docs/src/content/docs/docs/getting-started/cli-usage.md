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

## Web Searching

Use `--websearch` to choose the web-search policy.

```sh
thndrs --websearch duckduckgo
thndrs --websearch searxng --websearch-url http://127.0.0.1:8080
thndrs --websearch none
```

`duckduckgo` is the default. `none` disables application-owned web search.

## Prompt Inspection

Use `--print-prompt` to print the assembled system prompt bundle and lowered
provider messages without calling the provider.

```sh
thndrs --print-prompt
```

The output redacts secrets.

## Terminal Options

Use `--tick-rate-ms` to tune UI tick timing. The TUI writes completed messages
to the terminal's native scrollback and redraws only the active prompt and
streaming output. Use the terminal's mouse wheel, scrollbar, search, and text
selection as you would for ordinary shell output. `thndrs` does not capture the
mouse.

The prompt uses the terminal's blinking text cursor, including while editing a
multi-line prompt. Page Up and Page Down remain available inside focused
pickers and detail surfaces.
