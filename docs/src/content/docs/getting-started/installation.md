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

Run setup from the repository you want to use:

```sh
thndrs setup
thndrs
```

Setup detects the workspace, selected provider, config files, credential
status, session directory, and local search tools. It can write user or project
config and store provider credentials outside TOML.

Provider credentials can also be managed directly:

```sh
thndrs login opencode-zen
thndrs auth status
thndrs logout opencode-zen
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

If the TUI opens with a provider model but no usable credential, it shows a
focused recovery surface before submitting the first prompt. You can enter a
key, switch model/provider, show setup instructions, or quit without losing the
prompt draft.
