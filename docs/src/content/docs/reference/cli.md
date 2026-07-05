---
title: "CLI Reference"
---

## Options

- `--cwd <path>`: working directory for context loading and display.
- `--model <name>`: completion model.
- `--websearch <auto|native|exa|none>`: web search provider policy.
- `--verbose`: show diagnostic transcript rows such as provider events and log paths.
- `--theme <eldritch-minimal|iceberg-dark|catppuccin-mocha>`: UI color theme.
- `--mouse`: enable terminal mouse capture for overlay mouse events.
- `--no-alt-screen`: compatibility no-op; the TUI always renders inline without using the alternate screen.
- `--print-prompt`: print the assembled prompt bundle and exit without calling the provider.

## ACP Commands

- `thndrs acp list`: list configured ACP agents.
- `thndrs acp inspect <name>`: show one configured ACP agent with command values redacted.
- `thndrs acp smoke <name> --prompt <text>`: initialize an ACP agent, create a temporary session, send one prompt, and print streamed events.
- `thndrs acp logout <name>`: call agent-owned logout when advertised.
- `thndrs acp list-sessions <name>`: list external sessions owned by an ACP agent when it advertises `session/list`.
- `thndrs acp load-session <name> <session-id>`: load an external ACP session and print replayed `session/update` events.
- `thndrs acp resume-session <name> <session-id>`: resume an external ACP session without replaying history.
- `thndrs acp close-session <name> <session-id>`: close an external ACP session when the agent advertises `session/close`.

## MCP Commands

- `thndrs mcp list`: list configured MCP servers as
  `<server>\t<enabled|disabled>\t<transport>` and print config diagnostics.
- `thndrs mcp test <name>`: initialize one server and print
  `<server>\tready\t<N> tools`, followed by startup diagnostics.
- `thndrs mcp tools <name>`: list provider-visible namespaced tools from one
  server as `<mcp__server__tool>\t<description>`.
- `thndrs mcp call <server> <tool> --json <args>`: call one original MCP tool
  name with JSON object arguments and print capped tool output lines.

The `<tool>` argument for `mcp call` is the original MCP tool name reported by
the server. Provider-facing names are namespaced as `mcp__{server}__{tool}`.
