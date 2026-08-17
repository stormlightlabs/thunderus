---
title: "Environment Variables"
---

Environment variables serve two separate purposes:

- Ordinary configuration overrides use the `THNDRS_` prefix.
- Provider secrets use provider-specific variable names.

Unknown `THNDRS_` variables are errors. This catches misspelled overrides rather
than silently running with a lower-precedence value.

## Ordinary Config Overrides

These variables override TOML config files and built-in defaults. CLI flags
still override environment variables.

| Variable                   | Value                                                                        |
| -------------------------- | ---------------------------------------------------------------------------- |
| `THNDRS_MODEL`             | Completion model string.                                                     |
| `THNDRS_REASONING_EFFORT`  | Model-specific reasoning control:                                            |
|                            | `auto`, `on`, `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, or `max`. |
| `THNDRS_REASONING_SUMMARY` | `off` or `auto` for GPT-5.6 summaries.                                       |
| `THNDRS_TICK_RATE_MS`      | Positive integer milliseconds.                                               |
| `THNDRS_THEME`             | `eldritch-minimal`, `iceberg-dark`, or `catppuccin-mocha`.                   |
| `THNDRS_VERBOSE`           | Boolean                                                                      |
| `THNDRS_SKILL_DIRS`        | Path list using the platform path separator.                                 |
| `THNDRS_SESSION_DIR`       | Session JSONL directory path.                                                |
| `THNDRS_DEFAULT_WORKSPACE` | Workspace path used when `--cwd` is omitted.                                 |

Examples:

```sh
export THNDRS_MODEL=opencode/big-pickle
export THNDRS_VERBOSE=on
export THNDRS_SESSION_DIR="$HOME/.local/share/thndrs/sessions"
```

On Unix-like systems, path lists use `:`:

```sh
export THNDRS_SKILL_DIRS="$HOME/skills:$PWD/vendor/agent-skills"
```

On Windows, path lists use `;`.

## Provider Secrets

Provider secrets are not ordinary config keys. They may be read from the process
environment, managed credential stores, or a workspace `.env` file, but they are
not serialized into effective config, prompt inspection, sessions, snapshots, or
export metadata.

API-key provider credential lookup precedence is:

1. Process environment.
2. Global credential store: `~/.thndrs/credentials.env`.
3. Project credential store: `.thndrs/credentials.env`.
4. Workspace `.env` file for compatibility.

Use `thndrs login <provider>`, `thndrs logout <provider>`, and
`thndrs auth status` to manage stored provider credentials. Project credential
stores are added to `.git/info/exclude` when possible.

ChatGPT Codex is different: normal setup and login use ChatGPT OAuth and store
refreshable credentials in `~/.thndrs/auth.json`. The
`CHATGPT_CODEX_ACCESS_TOKEN` environment variable is only a process-local
override for automation and debugging.

### `OPENCODE_GO_KEY`

API key used by the OpenCode Go provider.

```sh
export OPENCODE_GO_KEY=sk-...
```

### `OPENCODE_ZEN_KEY`

API key used by the OpenCode Zen provider for `opencode/<model-id>` models such
as `opencode/big-pickle`.

```sh
export OPENCODE_ZEN_KEY=sk-...
```

`OPENCODE_ZEN_KEY` is separate from `OPENCODE_GO_KEY`; the two OpenCode provider
families use different model prefixes and credential slots.

### `CHATGPT_CODEX_ACCESS_TOKEN`

Ephemeral access-token override for `chatgpt-codex/<model-id>` models.

```sh
export CHATGPT_CODEX_ACCESS_TOKEN=...
```

This value is used only for the current process and is not written to
`~/.thndrs/auth.json`. It is not the normal setup path. Use
`thndrs setup --provider chatgpt-codex` or `thndrs login chatgpt-codex` to
create or refresh local ChatGPT OAuth credentials.

Do not use TOML keys such as `provider_api_key`, `auth_token`, `secret`,
or `password`. Secret-shaped TOML keys are rejected; provider code owns provider
secret loading and redaction.

[^1]: Boolean values accept `1`, `0`, `true`, `false`, `yes`, `no`, `on`, and `off`, case-insensitively.
