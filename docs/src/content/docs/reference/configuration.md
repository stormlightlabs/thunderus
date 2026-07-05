---
title: "Configuration"
---

`thndrs` uses built-in defaults, optional TOML config files, `THNDRS_`
environment variables, and CLI flags.

Precedence, from highest to lowest:

1. CLI flags.
2. `THNDRS_` environment variables.
3. Project config.
4. Global config.
5. Built-in defaults.

Secrets are provider-owned environment variables. Do not put API keys, tokens,
passwords, or secret values in TOML config.

## Config Files

TOML is the only config file format.

Supported paths:

- Global: `~/.thndrs/config.toml`
- Project: `.thndrs/config.toml`

No alternate spellings are supported. Files such as `.thndrs.toml`,
`.thndrs/thndrs.toml`, `.thndrs/.thndrs.toml`, and `.thdrs/*` are ignored.

Project config overrides global config. Unknown keys and malformed TOML are
errors.

## Keys

| Key                 | Type               | Default                             | Description                                    |
| ------------------- | ------------------ | ----------------------------------- | ---------------------------------------------- |
| `model`             | string             | `umans-coder`                       | Default completion model.                      |
| `websearch`         | `auto`             | `auto`                              | Web-search mode.                               |
|                     | `native`           |                                     |                                                |
|                     | `exa`,             |                                     |                                                |
|                     | `none`             |                                     |                                                |
| `tick_rate_ms`      | integer            | `100`                               | Event poll interval in milliseconds.           |
| `theme`             | `eldritch-minimal` | `eldritch-minimal`                  | UI color theme.                                |
|                     | `iceberg-dark`     |                                     |                                                |
|                     | `catppuccin-mocha` |                                     |                                                |
| `mouse`             | boolean            | `false`                             | Enable focused terminal mouse capture.         |
| `verbose`           | boolean            | `false`                             | Show diagnostic transcript rows.               |
| `skill_dirs`        | array of paths     | `[]`                                | Additional local skill discovery roots.        |
| `session_dir`       | path               | `.thndrs/sessions` in the workspace | Directory for append-only session JSONL files. |
| `default_workspace` | path               | current process directory           | Workspace used when `--cwd` is omitted.        |
| `acp_agents`        | table              | `{}`                                | Configured external ACP agents.                |

Relative `skill_dirs`, `session_dir`, and `default_workspace` values are
resolved relative to the config file that declares them.

Collection keys append across layers and then deduplicate by resolved path. For
`skill_dirs`, global entries come first, then project entries, then environment
entries, then CLI `--skill-dir` entries.

## Example

```toml
model = "umans-coder"
websearch = "auto"
tick_rate_ms = 100
theme = "eldritch-minimal"
mouse = false
verbose = false
skill_dirs = ["vendor/agent-skills"]
session_dir = ".thndrs/sessions"
default_workspace = ".."

[acp_agents.codex]
command = "npx"
args = ["-y", "@zed-industries/codex-acp@latest"]
env = {}
enabled = true
timeout_secs = 60
```

A standalone sample is available at
[`/thndrs-config.sample.toml`](/thndrs-config.sample.toml).

## CLI-Only Settings

These settings are intentionally CLI-only:

- `--cwd`: one-run workspace override.
- `--print-prompt`: print prompt assembly and exit.
- `--no-alt-screen`: compatibility no-op while the parser accepts it.
- `--no-mouse`: one-run override for `mouse = false`.

`cwd` is not a TOML or environment key because it controls which project config
file is discovered. Use `default_workspace` for a persistent workspace default,
and use `--cwd` when a single invocation needs to point somewhere else.

`print_prompt` is rejected in TOML and env because a persistent setting that
exits immediately would make normal startup surprising. Use `mouse = false`
instead of a persistent `no_mouse` key.

## Sessions

By default, sessions are written under the selected workspace:

```text
.thndrs/sessions/session-YYYYMMDD-HHMMSS.jsonl
```

Set `session_dir` to use another directory. Session metadata records safe
configuration metadata such as loaded config file paths, SHA-256 hashes, key
origins, effective model, web-search mode, workspace, and session directory.
It does not persist provider API keys or raw provider-private state.

## Web Search

Set `websearch` to choose the default search mode:

- `auto`: choose per prompt.
- `native`: use Umans server-side native search.
- `exa`: ask Umans to use its Exa-backed search path.
- `none`: disable Umans server-side search.

Repository file discovery is not configured through `websearch`; it is an
implementation detail of local read-only tools.

## MCP Servers

MCP servers use separate config files:

- Global: `~/.thndrs/mcp.toml`
- Project: `.thndrs/mcp.toml`

Project MCP server definitions override global definitions with the same server
name. Server names must match `[A-Za-z0-9_-]+`.

Example stdio server:

```toml
[servers.docs]
transport = "stdio"
command = "docs-mcp"
args = ["--workspace", "${THNDRS_WORKSPACE}"]
enabled = true
timeout_secs = 20
```

Example Streamable HTTP server:

```toml
[servers.search]
transport = "streamable_http"
url = "https://mcp.example.test/mcp"
headers = { authorization = "Bearer ${THNDRS_MCP_TOKEN}" }
timeout_secs = 20
```

Supported server keys are `transport`, `command`, `args`, `env`, `url`,
`headers`, `enabled`, and `timeout_secs`. `transport` defaults to `stdio`.
`command` is required for stdio; `url` is required for Streamable HTTP.

Environment expansion uses `${NAME}` inside values only. If a referenced
variable is missing, that server is skipped and a diagnostic is recorded. Secret
values in `env` and `headers` are redacted in loaded config metadata and
diagnostics.

See [MCP](/usage/mcp/) for stdio setup, Streamable HTTP examples, tool
namespacing, diagnostics, and security limits.

## ACP Agents

ACP agents are configured in the normal `thndrs` config files:

- Global: `~/.thndrs/config.toml`
- Project: `.thndrs/config.toml`

Project ACP agent definitions override global definitions with the same agent
name. Agent names must match `[A-Za-z0-9_-]+`.

```toml
[acp_agents.codex]
command = "npx"
args = ["-y", "@zed-industries/codex-acp@latest"]
env = {}
enabled = true
timeout_secs = 60
```

Supported agent keys are `command`, `args`, `env`, `enabled`, and
`timeout_secs`. `command` is required. `args` defaults to `[]`, `env` defaults
to `{}`, `enabled` defaults to `true`, and `timeout_secs` defaults to `60`.

ACP currently supports stdio agents only. The command is launched as a local
child process and must speak ACP JSON-RPC over stdin/stdout.

Environment values in `env` are passed to the child process and redacted in
diagnostics and session metadata.

Select a configured agent with `--model acp:<name>`. See [ACP](/usage/acp/) for
permission prompts, supported capabilities, troubleshooting, and ACP commands.

## Diagnostics

Startup fails for these configuration errors:

- Malformed TOML.
- Unknown TOML keys.
- Unknown `THNDRS_` environment variables.
- Invalid environment values, such as an invalid boolean or web-search mode.
- Secret-shaped TOML keys ending in `_api_key`, `_token`, `_secret`,
  `_password`, `secret`, or `password`.

Boolean environment values accept `1`, `0`, `true`, `false`, `yes`, `no`, `on`,
and `off`, case-insensitively.

Config path display uses workspace-relative paths for project config,
`~`-relative paths for global config under the home directory, and absolute
paths otherwise.

## Stability

`thndrs` has not reached its first stable release. Configuration and session
metadata may change before that release, but unsupported old or typo config
paths are intentionally ignored now so they do not become permanent public
contract.

See [Environment Variables](/reference/environment-variables/) for ordinary
`THNDRS_` overrides and provider secret variables.
