---
outline: deep
---

# CLI Reference

This page describes the current CLI surface based on the `thunderus` binary. The
CLI is evolving; run `thunderus --help` for the authoritative list.

## Global Usage

```
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

```
thunderus start [--dir DIR]
```

### `exec`

Execute a single command and exit (non-interactive mode).

```
thunderus exec <CMD> [ARGS...]
```

### `status`

Display the current configuration status and profile information.

```
thunderus status
```

### `completions`

Generate shell completion scripts.

```
thunderus completions <shell>
```

Supported shells are those exposed by `clap_complete` (for example: `bash`,
`zsh`, `fish`, `powershell`).

## Planned: Expanded CLI Surface

Additional subcommands for memory inspection, patch queue management, and
workflow shortcuts are on the roadmap. Treat those as **Planned** until they
land in `--help` output.
