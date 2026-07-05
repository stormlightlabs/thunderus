# thndrs ("Thunderus")

A minimal AI pair programmer.

![terminal hero showing the empty thndrs TUI](./docs/public/screenshot.png)

## Overview

**todo**

## Features

**todo**

### Terminal Pairing

**todo**

<!-- good screenshot: active conversation with user prompt, assistant response, reasoning row, and streaming status -->

### Project Context

**todo**

<!-- good screenshot: startup state showing loaded AGENTS.md/context status row -->

### Web Search

**todo**

<!-- good screenshot: search started/result rows in the transcript -->

### Themes

<!-- good screenshot: side-by-side terminal captures of eldritch-minimal, iceberg-dark, and catppuccin-mocha themes -->

| Eldritch | Iceberg  | Mocha    |
| -------- | -------- | -------- |
| **todo** | **todo** | **todo** |

## Quickstart

**todo**

## Installation

**todo**

## Configuration

`thndrs` reads an optional TOML config from exactly two paths:

- Global: `~/.thndrs/config.toml`
- Project: `.thndrs/config.toml`

Precedence is CLI flags over `THNDRS_` environment variables over project config
over global config over built-in defaults.

Supported config keys are `model`, `websearch`, `tick_rate_ms`, `theme`,
`mouse`, `verbose`, `skill_dirs`, `session_dir`, and `default_workspace`.
Provider secrets stay out of TOML; set `UMANS_API_KEY` or `OPENCODE_GO_KEY` in
the environment or workspace `.env` file.

Example:

```toml
model = "umans-coder"
websearch = "auto"
session_dir = ".thndrs/sessions"
default_workspace = "."
```

## Usage

**todo**

<!-- good screenshot: normal running session after a tool call, with prompt ready for follow-up -->

## Documentation

Public documentation is in Astro/Starlight [docs](/docs) project and is published
at https://thndrs.stormlightlabs.org.

## License

`thndrs` is licensed under the Apache License, Version 2.0.

See [`LICENSE`](./LICENSE) for the full license text.
