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
