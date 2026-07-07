# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows semantic versioning after the first stable release.

## [Unreleased]

### Added

- TOML configuration loading from `~/.thndrs/config.toml` and `.thndrs/config.toml`.
- `THNDRS_` environment overrides for ordinary runtime settings.
- Effective configuration metadata to startup diagnostics, prompt
  inspection, sessions, and inspect/export output.
- Configurable `session_dir` and `default_workspace` behavior.
- Registry-backed built-in tool catalog.
- MCP server configuration through user and project `mcp.toml` files.
  - Namespaced MCP tools using the `mcp__{server}__{tool}` naming scheme.
  - MCP CLI commands for listing servers, testing a server, listing tools,
    and calling a tool.
  - Streamable HTTP MCP support alongside stdio MCP servers.
- Configured ACP agents selectable with `--model acp:<name>`.
  - ACP permission prompts, filesystem callbacks, terminal callbacks,
    auth/logout handling, agent-owned session commands, registry discovery, and
    MCP-over-ACP config passing.
- OpenCode Zen provider support through `opencode/<model-id>` models.
  - `opencode/big-pickle` is available as the built-in default model.
  - `OPENCODE_ZEN_KEY` is handled separately from `OPENCODE_GO_KEY`.
  - OpenCode Zen model discovery feeds validation and picker refresh without
    inferring pricing from provider metadata.
- ChatGPT-backed Codex provider support through `chatgpt-codex/<model-id>`
  models.
  - Device-code login, browser PKCE fallback login, refreshable
    `~/.thndrs/auth.json` credentials, logout, and
    `CHATGPT_CODEX_ACCESS_TOKEN` process override.
  - TUI recovery can start ChatGPT OAuth, show the device-code verification
    URL and user code, poll without blocking prompt rendering, and cancel
    without writing credentials.
  - Known model picker entries for `gpt-5.5`, `gpt-5.4`, `gpt-5.4-mini`, and
    `gpt-5.3-codex-spark`.
- TTFT statusline display for client-observed time from local submit to first
  semantic model output.
- Semantic TUI view records for transcript rows, prompt state, orientation
  fields, focused surfaces, setup forms, structured tables, tool details, and
  diff summaries.
- Prompt interaction surfaces for command suggestions, inline `@` file
  mentions, `Ctrl+P` workspace file picking, `Ctrl+O` detail inspection, and
  queued steering/follow-up summaries while a turn is running.
- iocraft-backed bounded rendering for focused command picker, file picker, and
  help surfaces through the existing row/frame renderer contract.
- Structured markdown/table rendering with column width policies, alignment,
  header separators, selected-row styling, and narrow fallbacks.
- Ignored live provider smoke tests for OpenCode Zen and ChatGPT Codex
  credentials, streaming, tool calls, and refresh prerequisites.

### Changed

- Derived provider tool definitions from the tool registry instead of a central
  hand-maintained schema list.
- Moved built-in tool parsing, execution, and side-effect metadata into each
  tool's module boundary.
- Made configuration precedence explicit: CLI flags, environment variables,
  project config, global config, then defaults.
- Kept provider secrets outside ordinary TOML config and documented them
  separately from `THNDRS_` settings.
- Ignored unsupported legacy config path spellings before the first stable
  release.
- Preserved file-write and shell-execution audit metadata after the tool
  registry migration.
- Kept external MCP tool calls bounded, redacted, namespaced, and recorded in
  sessions.
- Kept ACP credentials agent-owned and out of config, logs, sessions, and
  inspect/export output.
- Moved ACP remote/custom transport planning into the ACP agent server feature.
- Changed the built-in default model from `umans-coder` to
  `opencode/big-pickle`, with setup and docs preserving OpenCode's
  limited-free and free-period privacy caveats.
- Reworked `thndrs setup` into a provider-aware flow with an explicit provider
  picker, OpenCode Zen Big Pickle as the default choice, hidden API-key entry
  for API-key providers, and ChatGPT OAuth for `chatgpt-codex`.
- Kept `opencode/` and `opencode-go/` as distinct OpenCode provider families
  with separate credentials, routing, and docs.
- Labeled ChatGPT Codex as ChatGPT-backed and experimental instead of treating
  it as OpenAI Platform API-key access.
- Kept ChatGPT Codex setup and login on the same OAuth path; normal setup does
  not ask for ChatGPT API keys and does not store ChatGPT credentials in
  `.thndrs/credentials.env`.
- Kept provider secrets, raw provider payloads, ChatGPT access tokens, refresh
  tokens, and TTFT content out of session records and prompt inspection.
- Kept the committed transcript, main prompt editor, permission prompts,
  first-run recovery, and tool detail rendering on the direct row path where
  it preserves native scrollback, priority rules, cursor behavior, and richer
  detail semantics.
- Documented the TUI trust wording as
  `local user · workspace-contained tools · no TUI sandbox` without implying a
  stronger sandbox than the runtime enforces.
