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

## Auth Commands

- `thndrs setup [--provider <umans|opencode-go|opencode-zen|chatgpt-codex>]`: run guided setup for the selected provider. API-key providers use hidden key entry; `chatgpt-codex` uses ChatGPT OAuth.
- `thndrs setup --global`: prefer the user config and credential store.
- `thndrs setup --project`: prefer the workspace config and credential store.
- `thndrs login <provider>`: store an API-key provider credential after hidden input and explicit confirmation.
- `thndrs logout <provider>`: remove the selected provider's stored credential.
- `thndrs auth status`: show provider credential sources without values.
- `thndrs doctor`: print redacted human-readable setup diagnostics.
- `thndrs doctor --json`: print redacted machine-readable diagnostics for bug reports.
- `thndrs config path`: print global and project config paths.
- `thndrs config show --redacted`: print effective config, origins, loaded files, and diagnostics.
- `thndrs config edit --global`: open the global config file with `$EDITOR`.
- `thndrs config edit --project`: open the project config file with `$EDITOR`.
- `thndrs login chatgpt-codex`: start the ChatGPT Codex device-code login flow, with browser PKCE fallback.
- `thndrs logout chatgpt-codex`: remove the stored ChatGPT Codex credential entry.

Supported API-key setup and login providers include `umans`, `opencode-go`, and
`opencode-zen`. ChatGPT Codex setup and login share the ChatGPT-backed OAuth
flow and store refreshable credentials in `~/.thndrs/auth.json`, not in
`.thndrs/credentials.env`.

## ACP Commands

ACP agents are selected in the TUI with `--model acp:<name>`. They are
configured under `[acp_agents.<name>]`. For more information see [ACP](/usage/acp/).

- `thndrs acp list`: list configured ACP agents.
- `thndrs acp inspect <name>`: show one configured ACP agent with command values redacted.
- `thndrs acp smoke <name> --prompt <text>`: initialize an ACP agent, create a temporary session, send one prompt, and print streamed events.
- `thndrs acp logout <name>`: call agent-owned logout when advertised.
- `thndrs acp list-sessions <name>`: list external sessions owned by an ACP agent when it advertises `session/list`.
- `thndrs acp load-session <name> <session-id>`: load an external ACP session and print replayed `session/update` events.
- `thndrs acp resume-session <name> <session-id>`: resume an external ACP session without replaying history.
- `thndrs acp close-session <name> <session-id>`: close an external ACP session when the agent advertises `session/close`.
- `thndrs acp registry [--file <path>]`: list agents from the read-only official ACP Registry metadata without installing them.
- `thndrs acp install <registry-id> [--name <name>] [--file <path>] --yes`: add a supported registry agent to workspace ACP config and installed-agent metadata.
- `thndrs acp update <name> [--file <path>] --yes`: update a registry-managed ACP agent in workspace ACP config and installed-agent metadata.

## ACP Agent Server

`thndrs-acp-server` exposes `thndrs` as an ACP stdio agent for editors and IDEs.
stdout is protocol-only; diagnostics go to stderr.

- `thndrs-acp-server --cwd <path>`: start the ACP server for a workspace.
- `--model <model>`: provider model for new sessions.
- `--websearch <auto|native|exa|none>`: web search provider policy.
- `--session-dir <path>`: append-only local session JSONL directory.

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
