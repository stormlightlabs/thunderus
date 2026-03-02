<!-- markdownlint-disable MD033 -->
# Thunderus AI Agent

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
