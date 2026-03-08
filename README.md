<!-- markdownlint-disable MD033 -->
# Thunderus AI Agent

Thunderus is a coding agent harness built in Rust. It attempts to replicate and build upon the workflows of pi, OpenCode, Claude Code and Codex, as a standalone, provider-agnostic TUI tool.

It is designed for developers who want to use open-source/open-weight models with fine
grained control over context.

## Project Structure

```sh
.
├── crates/
│   ├── cli/         # CLI entry point, clap, owo-colors
│   ├── core/        # Core logic, conversation state, message handling
│   ├── memory/      # Memory management, SQLite, embeddings
│   ├── providers/   # LLM provider integrations
│   └── tools/       # Tool definitions and execution
├── designs/         # Design mockups and templates
├── meta/            # Source of truth (prompts, tools, response format)
└── docs/            # Documentation, specs, provider details
```

## Configuration

Thunderus loads config from `~/.thunderus/config.toml` by default. You can override with `--config <path>`.

<details>
<summary>Example Config</summary>

```toml
default_provider = "moonshot"
default_model = "kimi-k2.5"
temperature = 0.7
max_tokens = 4096

[providers.moonshot]
api_key = "sk-..."
base_url = "https://api.moonshot.ai/v1"
default_model = "kimi-k2.5"

[providers.zhipu]
api_key = "id.secret"
base_url = "https://api.z.ai/api/coding/paas/v4"
default_model = "glm-5"
```

### Notes

- `default_provider` controls which backend the TUI uses on startup (`moonshot`/`kimi` or `zhipu`/`glm`).
- `temperature` is clamped to `[0.0, 1.0]` for Moonshot and Zhipu providers.
- If no default config file exists, Thunderus falls back to built-in defaults and provider calls will fail until API keys are configured.

</details>

## Logs

<details>
<summary>
Viewing Logs and Log Files
</summary>

Thunderus writes logs in two places:

- Session logs in the workspace SQLite database (viewable inside the TUI)
- Runtime tracing logs in rotating text log files (viewable from CLI)

</details>

<details>
<summary>
Session Logs (inside TUI)
</summary>

1. Start Thunderus.
2. Run `/history` to list saved session IDs.
3. Run `/debug log <session-id>` to view logs for that session.

Example:

```text
/history
/debug log 8c5d9f8b-...
```

</details>

<details>
<summary>
Runtime Log Files (CLI)
</summary>

Use the debug commands from the workspace root:

```sh
thunderus debug tail --lines 120
thunderus debug attach --lines 120 --poll-ms 250
```

If you run via Cargo during development:

```sh
cargo run -p cli -- debug tail --lines 120
cargo run -p cli -- debug attach --lines 120 --poll-ms 250
```

`debug tail` prints recent lines, and `debug attach` follows new log output.

</details>

<details>
<summary>
Log Files
</summary>

Runtime logs are stored under:

```text
~/.thunderus/logs/workspaces/<workspace-hash>/runtime.log*
```

Workspace hash is the first 16 characters of SHA-256 of the absolute workspace path:

```sh
workspace_hash="$(printf '%s' "$(pwd)" | shasum -a 256 | awk '{print substr($1,1,16)}')"
echo "$HOME/.thunderus/logs/workspaces/$workspace_hash"
```

</details>
