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

| Variable                   | Value                                                      |
| -------------------------- | ---------------------------------------------------------- |
| `THNDRS_MODEL`             | Completion model string.                                   |
| `THNDRS_WEBSEARCH`         | `auto`, `native`, `exa`, or `none`.                        |
| `THNDRS_TICK_RATE_MS`      | Positive integer milliseconds.                             |
| `THNDRS_THEME`             | `eldritch-minimal`, `iceberg-dark`, or `catppuccin-mocha`. |
| `THNDRS_MOUSE`             | Boolean.                                                   |
| `THNDRS_VERBOSE`           | Boolean.                                                   |
| `THNDRS_SKILL_DIRS`        | Path list using the platform path separator.               |
| `THNDRS_SESSION_DIR`       | Session JSONL directory path.                              |
| `THNDRS_DEFAULT_WORKSPACE` | Workspace path used when `--cwd` is omitted.               |

Boolean values accept `1`, `0`, `true`, `false`, `yes`, `no`, `on`, and `off`,
case-insensitively.

Examples:

```sh
export THNDRS_MODEL=umans-coder
export THNDRS_WEBSEARCH=auto
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
environment or from a workspace `.env` file, but they are not serialized into
effective config, prompt inspection, sessions, snapshots, or export metadata.

### `UMANS_API_KEY`

API key used by the Umans provider.

```sh
export UMANS_API_KEY=sk-...
```

### `OPENCODE_GO_KEY`

API key used by the OpenCode Go provider.

```sh
export OPENCODE_GO_KEY=sk-...
```

Do not use TOML keys such as `umans_api_key`, `auth_token`, `secret`, or
`password`. Secret-shaped TOML keys are rejected; provider code owns provider
secret loading and redaction.
