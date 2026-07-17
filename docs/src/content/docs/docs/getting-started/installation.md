---
title: "Installation"
---

## Requirements

- Rust toolchain.
- A terminal with Unicode support.
- `fd` and `rg` for the read-only repository tools.

## Install

```sh
cargo install --locked thndrs
```

## First Run

Run `thndrs` from the repository you want to use:

```sh
thndrs
```

On a fresh install, the application opens required setup before it accepts a
coding prompt. Choose a provider and authenticate there. `thndrs setup` offers
the same workflow from the CLI.

Provider credentials can also be managed directly:

```sh
thndrs setup --provider chatgpt-codex
thndrs setup --provider umans
thndrs auth status
```

Secrets are not accepted through CLI flags or TOML config. Use `thndrs login`,
`thndrs setup`, or provider-specific environment variables.

## Troubleshooting

Run diagnostics before filing an issue:

```sh
thndrs doctor
thndrs doctor --json
```

`doctor --json` is safe to paste into bug reports. It reports config files,
credential sources, tool availability, session directory status, MCP/ACP
counts, and blocking setup issues without printing credential values.

If a provider rejects a credential, `thndrs` keeps the prompt out of the coding
path and points to the appropriate login action.

A network or service failure will ask you to retry setup instead.

Local tools run with the permissions of the user who started `thndrs`. Use a container,
VM, or OS-level sandbox when the task needs isolation.
