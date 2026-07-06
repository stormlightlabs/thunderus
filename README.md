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
local config, stores provider credentials outside TOML, and prints the next
command to run.

## Configuration

See the doc site for the up to date [configuration reference](https://thndrs.stormlightlabs.org/reference/configuration/)

## Usage

**todo**

<!-- good screenshot: normal running session after a tool call, with prompt ready for follow-up -->

## Documentation

Public documentation is in an Astro/Starlight [project](/docs) and is published at
https://thndrs.stormlightlabs.org.

## License

`thndrs` is licensed under the Apache License, Version 2.0.

See [`LICENSE`](./LICENSE) for the full license text.
