# CHANGELOG

## Unreleased

### Added

- Provider-neutral context accounting with exact serialized bytes,
  conservative token estimates, provider-reported usage, cache components,
  and explicit measurement provenance.
- Separate durable-evidence, user-display, and model-projection contracts for
  tool results, with bounded redacted artifacts and recovery handles.
- Inspectable context lifecycle, protection, verification, reduction receipts,
  and deterministic JSON and Markdown exports.
- Preservation-tested terminal, repetition, duplicate-evidence, and command
  result reducers with frozen replay fixtures and projection benchmarks.
- Review-gated range compression with source provenance, protected-fact
  preservation, atomic rejection, and bounded recovery.
- Context compaction through `/compact` or automatic budget thresholds, with
  anchored summaries, a retained recent turn-aligned tail, and configurable
  mode, review policy, threshold, and tail size.

### Changed

- Refined the TUI with responsive content rails, a quieter conversation
  hierarchy, a unified composer and status area, grouped tool activity, and
  semantic colors across the built-in themes.

## v0.1.0

This release introduces `thndrs` as experimental pre-1.0 software. Its CLI,
configuration, provider, session, and tool behavior may change during the
pre-1.0 release line.

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
  - Known model picker entries for `gpt-5.6` (sol, terra, & luna) `gpt-5.5`,
    `gpt-5.4`, `gpt-5.4-mini`, and `gpt-5.3-codex-spark`.
- Model-specific reasoning effort and summary controls where a provider
  supports them.
- TTFT statusline display for client-observed time from local submit to first
  semantic model output.
- Prompt interaction surfaces for command suggestions, inline `@` file
  mentions, `Ctrl+P` workspace file picking, `Ctrl+O` detail inspection, and
  queued steering/follow-up summaries while a turn is running.
- Redacted diagnostics, prompt inspection, configuration provenance, and
  session inspection/export.
- Web search tool with configurable backend (DuckDuckGo or SearXNG).
