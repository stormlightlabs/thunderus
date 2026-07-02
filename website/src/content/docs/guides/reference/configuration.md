---
title: "Configuration"
---

`thndrs` uses CLI flags, environment variables, and optional TOML config files.
CLI flags override config files.

Current user-controlled settings:

- `--cwd`: workspace root.
- `--model`: completion model.
- `--websearch`: web-search mode.
- `--tick-rate-ms`: event poll interval.
- `--verbose`: diagnostic transcript rows.
- `--theme`: UI color theme.
- `--mouse` / `--no-mouse`: terminal mouse capture.
- `--no-alt-screen`: compatibility no-op; inline direct rendering is always used.
- `--print-prompt`: inspect prompt assembly without contacting the provider.

Secrets are read from environment variables, not config examples. See
[Environment Variables](./environment-variables.md).

## Config Files

Config is loaded in this order:

- Global config:
  - `~/.thndrs.toml`
  - `~/.thndrs/config.toml`
  - `~/.thndrs/.thndrs.toml`
  - `~/.thndrs/thndrs.toml`
- Project config:
  - `.thndrs.toml`
  - `.thndrs/config.toml`
  - `.thndrs/.thndrs.toml`
  - `.thndrs/thndrs.toml`
  - `.thdrs/config.toml`
  - `.thdrs/.thndrs.toml`
  - `.thdrs/thndrs.toml`

Project config overrides global config. Unknown keys and malformed TOML are
errors.

Example:

```toml
model = "umans-coder"
websearch = "auto"
tick_rate_ms = 100
mouse = false
verbose = false
theme = "catppuccin-mocha"
```

`cwd` is CLI-only because it controls which project config file is discovered.

Supported themes are `eldritch-minimal`, `iceberg-dark`, and
`catppuccin-mocha`.
