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
