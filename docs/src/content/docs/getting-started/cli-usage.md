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
thndrs --model umans-coder
```

Built-in provider model prefixes include:

- `chatgpt-codex/<model-id>`, for ChatGPT-backed Codex
- `umans-coder`
- `umans-glm-5.2`
- `opencode/<model-id>`, for OpenCode Zen
- `opencode-go/<model-id>`, for OpenCode Go

ChatGPT Codex and Umans are the first-class v0.1 workflows. OpenCode providers
and configured ACP agents are advanced integrations.

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

Use `--tick-rate-ms` to tune UI tick timing. The TUI uses an alternate screen
with an application-owned transcript.

Page Up and Page Down scroll by a page; Alt+Up and Alt+Down scroll by a line;
Ctrl+Home and Ctrl+End jump to the oldest entry and the live tail.

Use `--mouse` to enable transcript wheel scrolling and mouse navigation in pickers.

Use `--no-mouse` to disable capture. Most terminal emulators let you hold a modifier
while selecting text when capture is enabled.
