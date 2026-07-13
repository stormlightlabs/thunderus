# thndrs ("Thunderus")

A minimal AI pair programmer.

![terminal hero showing the empty thndrs TUI](./docs/public/screenshot.png)

## Overview

`thndrs` is an agentic coding harness for working with repository-aware
large language models.

## Features

- Support for ChatGPT COdex, Umans, OpenCode Go & Zen LLM providers.
- Native terminal scrollback, structured transcript events, session recovery,
  redacted diagnostics, and configurable reasoning controls.
- Workspace-contained file, search, URL, and shell tools with explicit approval
  for side effects.

### Terminal Pairing

**todo**

<!-- good screenshot: active conversation with user prompt, assistant response, reasoning row, and streaming status -->

### Project Context

`thndrs` discovers repository guidance such as `AGENTS.md` and skills (`SKILL.md`),
assembles bounded context, and exposes context inspection and recovery controls
inside the TUI.

<!-- good screenshot: startup state showing loaded AGENTS.md/context status row -->

### Web Search

Choose automatic, provider-native, Exa-backed, or disabled provider-side web
search with `--websearch` or `THNDRS_WEBSEARCH`.

### Themes

The TUI supports Eldritch Minimal[^eld], Iceberg[^ice], and Catppuccin[^cat] Mocha palettes.

## Quickstart

```sh
cargo install --locked thndrs
thndrs setup
thndrs
```

## Installation

Install from crates.io:

```sh
cargo install --locked thndrs
```

Then run `thndrs setup` from the repository you want to work in. Setup checks
local config, stores provider credentials outside the TOML, and prints the next
command to run.

## Configuration

See the doc site for the up to date [configuration reference](https://thndrs.stormlightlabs.org/reference/configuration/)

## Usage

**todo**

## Documentation

Public documentation is in an Astro/Starlight [project](/docs) and is published at
<https://thndrs.stormlightlabs.org>.

## License

`thndrs` is licensed under the Apache License, Version 2.0.

See [`LICENSE`](./LICENSE) for the full license text.

[^eld]: Eldritch.nvim theme <https://github.com/eldritch-theme/eldritch.nvim>

[^ice]: Iceberg.vim theme <https://github.com/cocopon/iceberg.vim>

[^cat]: Catppuccin palette <https://catppuccin.com/>
