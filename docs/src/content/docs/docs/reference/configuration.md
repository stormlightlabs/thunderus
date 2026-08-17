---
title: "Configuration"
---

`thndrs` uses required first-run setup, optional TOML config files, `THNDRS_`
environment variables, and CLI flags. A fresh install has no completion model
so setup records the provider and model before a coding prompt can run.

Precedence, from highest to lowest:

1. CLI flags.
2. `THNDRS_` environment variables.
3. Project config.
4. Global config.
5. Built-in defaults.

Secrets are provider- or agent-owned values. Do not put API keys, tokens,
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

## Provider Credentials

TOML config is for ordinary runtime settings. Provider credentials live in the
process environment, managed credential stores, provider-owned auth files, or an
agent's own auth flow.

API-key providers can use:

- Global credential store: `~/.thndrs/credentials.env`
- Project credential store: `.thndrs/credentials.env`
- Process environment variables such as `OPENCODE_ZEN_KEY`

Use `thndrs setup`, `thndrs login <provider>`, `thndrs logout <provider>`, and
`thndrs auth status` for local credential management. See
[Environment Variables](/docs/reference/environment-variables/) for precedence and
provider-specific names.

## Keys

| Key                 | Type                               | Default                             | Description                                                               |
| ------------------- | ---------------------------------- | ----------------------------------- | ------------------------------------------------------------------------- |
| `model`             | string                             | set by setup                        | Completion model override.                                                |
| `reasoning_effort`  | model-specific:    | `auto`                              | Reasoning control; unsupported choices fail locally.                      |
|                     | `auto`, `on`,      |                                     | Supported providers expose model-specific effort levels.                 |
|                     | `none`, `minimal`, |                                     |                                                                           |
|                     | `low`–`max`        |                                     |                                                                           |
| `reasoning_summary` | `off`, `auto`      | `off`                               | Whether GPT-5.6 summaries are shown.                                      |
| `tick_rate_ms`      | integer            | `33`                                | Event poll interval in milliseconds; values below `33` use `33`.          |
| `theme`             | `eldritch-minimal` | `eldritch-minimal`                  | UI color theme.                                                           |
|                     | `iceberg-dark`     |                                     |                                                                           |
|                     | `catppuccin-mocha` |                                     |                                                                           |
| `verbose`           | boolean            | `false`                             | Show diagnostic transcript rows.                                          |
| `skill_dirs`        | array of paths     | `[]`                                | Additional local skill discovery roots.                                   |
| `session_dir`       | path               | `.thndrs/sessions` in the workspace | Directory for append-only session JSONL files.                            |
| `default_workspace` | path               | current process directory           | Workspace used when `--cwd` is omitted.                                   |
| `acp_agents`        | table              | `{}`                                | Configured external ACP agents.                                           |
| `context`           | table              | see below                           | Context compaction and deterministic reduction settings.                  |
| `session_retention` | table              | see below                           | Automatic session retention and trash-grace settings.                     |

Relative `skill_dirs`, `session_dir`, and `default_workspace` values are
resolved relative to the config file that declares them.

Collection keys append across layers and then deduplicate by resolved path. For
`skill_dirs`, global entries come first, then project entries, then environment
entries, then CLI `--skill-dir` entries.

## Example

```toml
# Setup records the initial provider and model. Uncomment to override it.
# model = "opencode/big-pickle"
reasoning_effort = "auto"
reasoning_summary = "off"
tick_rate_ms = 33
theme = "eldritch-minimal"
verbose = false
skill_dirs = ["vendor/agent-skills"]
session_dir = ".thndrs/sessions"
default_workspace = ".."

[context.compaction]
mode = "manual"
review = "auto"
threshold = "92%"
keep_recent_tokens = 20000

[context.reduction]
shadow = true
terminal_control = false
progress_redraw = false
blank_run = false
repeated_line = false
state_identical = false
command_result = false
failed_tool_input = false
max_blank_lines = 1

[session_retention]
enabled = true
max_age_days = 30
max_live_count = 200
min_age_days = 1
trash_retention_days = 7

[acp_agents.codex]
command = "npx"
args = ["-y", "@zed-industries/codex-acp@latest"]
env = {}
enabled = true
timeout_secs = 60
```

A standalone sample is available at
[`/thndrs-config.sample.toml`](/thndrs-config.sample.toml).

## Context Compaction

`/compact` is available when `context.compaction.mode` is `manual` or `auto`.
Set the mode to `off` to disable both explicit and automatic compaction. In
`auto` mode, thndrs checks the projected request before sending it and compacts
first when it exceeds `threshold` of the available input budget. The available
input budget already accounts for the selected model's context window and
completion reserve.

```toml
[context.compaction]
mode = "auto"              # off | manual | auto
review = "auto"            # always | auto | never
threshold = "92%"          # 1% through 100%
keep_recent_tokens = 20000  # approximate token target
```

`threshold` uses percentage syntax so it remains model-aware as the selected
model changes. Compaction starts only after the projected request exceeds the
resolved threshold. `keep_recent_tokens` is an approximate target: thndrs cuts
only at a user-turn boundary, so the retained tail may be larger. An explicit
`/compact` still compacts a small closed history rather than refusing because
the history is below that target.

`review = "auto"` pauses only summaries that cover tool output, failures,
permissions, corrections, or unresolved work. Resolve them with `/context
review approve` or `/context review reject`. `always` pauses every summary;
`never` applies a valid typed summary immediately.

Compaction is separate from deterministic tool-output reduction. The latter is
configured under `[context.reduction]` and runs without asking a model. The
selected provider model performs semantic compaction; there is no separate
compaction backend setting.

## Context Reduction

Projection reducers operate on bounded model-facing tool evidence. They do not
rewrite session records or durable artifacts. Each reducer records a receipt
with its version, measurements, mode, and preservation result.

```toml
[context.reduction]
shadow = true
terminal_control = false
progress_redraw = false
blank_run = false
repeated_line = false
state_identical = false
command_result = false
failed_tool_input = false
max_blank_lines = 1
```

`shadow = true` measures disabled reducers without changing provider requests.
It is the default. Each application switch defaults to `false`:

- `terminal_control` removes ANSI and terminal control sequences while retaining
  rendered text.
- `progress_redraw` retains the final value from carriage-return redraws.
- `blank_run` limits consecutive blank lines to `max_blank_lines`, which must be
  at most `64`.
- `repeated_line` replaces consecutive exact non-blank repetitions with one
  counted line.
- `state_identical` suppresses repeated evidence only when the tool adapter
  supplies a matching state fingerprint.
- `command_result` uses an application-owned structured projection for a
  completed command when its operational evidence can be preserved.
- `failed_tool_input` removes oversized failed non-command tool arguments from
  the request only after bounded recovery evidence has been persisted.

There are no bundled reduction presets. Enable only the mechanisms you want to
apply. `/context export` includes shadow, applied, and baseline-fallback
receipts for the selected request.

## Session Retention

Automatic collection uses `[session_retention]`:

```toml
[session_retention]
enabled = true
max_age_days = 30
max_live_count = 200
min_age_days = 1
trash_retention_days = 7
```

At most once every 24 hours, collection selects unprotected live sessions older
than `max_age_days` or in excess of `max_live_count`. It never selects the
active session, pinned or locked sessions, corrupt records, or sessions younger
than `min_age_days`. Selected sessions move to recoverable trash. Trash becomes
eligible for permanent deletion after `trash_retention_days`.

Set `enabled = false` to disable policy-driven live-session pruning. Explicit
`thndrs sessions prune` overrides can still select sessions. The collection
pass also removes expired trash, unreferenced artifacts, orphan session state,
stale temporary files, and excess logs. Use `thndrs sessions storage` and
`thndrs sessions prune --dry-run` to inspect the effect first.

`min_age_days` must not exceed `max_age_days`.

## CLI-Only Settings

These settings are intentionally CLI-only:

- `--cwd`: one-run workspace override.
- `--print-prompt`: print prompt assembly and exit.

`cwd` is not a TOML or environment key because it controls which project config
file is discovered. Use `default_workspace` for a persistent workspace default,
and use `--cwd` when a single invocation needs to point somewhere else.

`print_prompt` is rejected in TOML and env because a persistent setting that
exits immediately would make normal startup surprising.

## Sessions

By default, sessions are written under the selected workspace:

```text
.thndrs/sessions/session-YYYYMMDD-HHMMSS.jsonl
```

Set `session_dir` to use another directory. Session metadata records safe
configuration metadata such as loaded config file paths, SHA-256 hashes, key
origins, effective model, workspace, and session directory.
It does not persist provider API keys or raw provider-private state.

## Web search

Configure web search through an MCP server. The normal configuration file holds
MCP connection settings separately from provider settings; see
[Web search and URL reading](/docs/usage/web-search/).

## MCP Servers

MCP servers use separate config files:

- Global: `~/.thndrs/mcp.toml`
- Project: `.thndrs/mcp.toml`

Project MCP configuration is inactive until it is trusted with `thndrs mcp
trust`. Once trusted, project server definitions override global definitions
with the same server name. Trust is tied to the workspace and exact file hash,
so editing `.thndrs/mcp.toml` blocks its servers until the new contents are
trusted. Use `thndrs mcp status` to inspect the decision and `thndrs mcp revoke`
to remove it.

`thndrs` reads MCP configuration but does not install server packages. Use
`thndrs mcp add <name> --scope <global|project> --command <command>` for a
stdio connection, adding one `--arg <arg>` for each argument, or use `--url`
for a Streamable HTTP connection. The command writes configuration only; it
does not install, start, or contact the server. Use `thndrs mcp remove <name>
--scope <global|project>` to remove a definition.

The guided commands do not accept headers, environment values, or tokens.
Configure credentials through environment references in manual TOML instead.
They preserve unrelated definitions and comments, validate the complete file,
and replace it atomically. Server names must match `[A-Za-z0-9_-]+`.

Example stdio server:

```toml
[servers.docs]
transport = "stdio"
command = "docs-mcp"
args = ["--workspace", "${PROJECT_ROOT}"]
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

See [MCP](/docs/usage/mcp/) for stdio setup, Streamable HTTP examples, tool
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

Use ACP `env` for non-secret child process settings. Values in `env` are passed
to the child process and redacted in diagnostics and session metadata, but
secret-shaped keys are still rejected anywhere in TOML, including under
`acp_agents.<name>.env`. Put agent credentials in the parent process
environment, the agent's own login/auth flow, or an explicit wrapper command
outside `thndrs` TOML.

Select a configured agent with `--model acp:<name>`. See [ACP](/docs/usage/acp/) for
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

See [Environment Variables](/docs/reference/environment-variables/) for ordinary
`THNDRS_` overrides and provider secret variables.
