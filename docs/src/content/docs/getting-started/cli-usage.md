---
title: "CLI Usage"
---

## Running the App

Running `thndrs` without a subcommand launches the TUI.

```sh
cargo run
```

## Working Directory

Use `--cwd` to select the workspace used for context loading, display, and
read-only tools.

```sh
cargo run -- --cwd /path/to/repo
```

## Model Selection

The default model is `opencode/big-pickle`.

```sh
cargo run -- --model umans-coder
```

Built-in provider model prefixes include:

- `opencode/<model-id>`, including `opencode/big-pickle`
- `opencode-go/<model-id>`, for OpenCode Go
- `chatgpt-codex/<model-id>`, for experimental ChatGPT-backed Codex
- `umans-coder`
- `umans-glm-5.2`

## Web Searching

Use `--websearch` to choose the web-search policy.

```sh
cargo run -- --websearch native
cargo run -- --websearch exa
cargo run -- --websearch none
```

`auto` is the default. `none` disables provider-side web search.

## Prompt Inspection

Use `--print-prompt` to print the assembled prompt bundle and lowered provider
messages without calling the provider.

```sh
cargo run -- --print-prompt
```

The output redacts secrets.

## Terminal Options

Use `--tick-rate-ms` to tune UI tick timing. The TUI always renders inline
without entering the alternate screen; `--no-alt-screen` is kept as a
compatibility no-op. Use `--no-mouse` to leave terminal mouse selection and
native scrollback uncaptured.
