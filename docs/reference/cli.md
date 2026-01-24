---
outline: deep
---

# CLI Reference

This page describes the current CLI surface based on the `thunderus` binary. The
CLI is evolving; run `thunderus --help` for the authoritative list.

## Global Usage

```sh
thunderus [--config PATH] [--profile PROFILE] [--verbose] [--dir DIR] [command]
```

### Global Flags

- `--config`, `-c`: Path to `config.toml`. Defaults to `./config.toml`.
- `--profile`, `-p`: Profile name to use. Defaults to `default_profile`.
- `--verbose`, `-v`: Enable verbose logging.
- `--dir`, `-d`: Working directory used by default start behavior.

## Commands

### `start`

Start the interactive TUI session.

```sh
thunderus start [--dir DIR]
```

### `exec`

Execute a single command and exit (non-interactive mode).

```sh
thunderus exec <CMD> [ARGS...]
```

### `status`

Display the current configuration status and profile information.

```sh
thunderus status
```

### `completions`

Generate shell completion scripts.

```sh
thunderus completions <shell>
```

Supported shells are those exposed by [`clap_complete`](https://crates.io/crates/clap_complete)
(for example: `bash`, `zsh`, `fish`, `powershell`).
