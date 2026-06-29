# CLI Usage

## Running the App

Running `thndrs` without a subcommand launches the TUI.

```sh
cargo run
```

## Working Directory

Use `--cwd` to select the workspace used for context loading, display, and
read-only tools.

```sh
cargo run -- --cwd /path/to/repo
```

## Model Selection

The default model is `umans-coder`.

```sh
cargo run -- --model umans-glm-5.2
```

Supported model names are currently:

- `umans-coder`
- `umans-glm-5.2`

## Web Searching

Use `--websearch` to choose the Umans web-search backend.

```sh
cargo run -- --websearch native
cargo run -- --websearch exa
cargo run -- --websearch none
```

`native` is the default. `none` disables Umans server-side search.

## Prompt Inspection

Use `--print-prompt` to print the assembled prompt bundle and lowered provider
messages without calling the provider.

```sh
cargo run -- --print-prompt
```

The output redacts secrets.

## Terminal Options

Use `--tick-rate-ms` to tune UI tick timing. Use `--no-alt-screen` when you want
the TUI to avoid entering the terminal alternate screen.
