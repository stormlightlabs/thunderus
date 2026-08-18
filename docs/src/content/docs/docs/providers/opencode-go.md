---
title: "OpenCode Go Provider"
---

OpenCode Go uses model ids in the `opencode-go/<model-id>` form:

```toml
model = "opencode-go/kimi-k2.7-code"
```

## Authentication

Set `OPENCODE_GO_KEY` in the process environment or workspace `.env` file, or
use the managed credential flow:

```sh
thndrs setup --provider opencode-go
thndrs login opencode-go
thndrs logout opencode-go
```

Provider keys are not accepted through CLI flags or TOML config. `OPENCODE_GO_KEY`
is separate from `OPENCODE_ZEN_KEY`.

## Models

The curated OpenCode Go model entries are:

- `opencode-go/grok-4.5`
- `opencode-go/glm-5.3`
- `opencode-go/glm-5.2`
- `opencode-go/glm-5.1`
- `opencode-go/gpt-5.6-luna`
- `opencode-go/kimi-k3`
- `opencode-go/kimi-k2.7-code`
- `opencode-go/kimi-k2.6`
- `opencode-go/mimo-v2.5`
- `opencode-go/mimo-v2.5-pro`
- `opencode-go/minimax-m3`
- `opencode-go/minimax-m2.7`
- `opencode-go/minimax-m2.5`
- `opencode-go/qwen3.8-max`
- `opencode-go/qwen3.7-max`
- `opencode-go/qwen3.7-plus`
- `opencode-go/qwen3.6-plus`
- `opencode-go/deepseek-v4-pro`
- `opencode-go/deepseek-v4-flash`
- `opencode-go/hy3`

`thndrs` also loads the authenticated `/models` response for picker metadata,
validation, and additional live model entries. The app-facing ids keep the
`opencode-go/` prefix; requests send the raw provider model id.

## Endpoint routing

OpenCode Go selects the endpoint from the raw model id:

| Model ids            | Endpoint                           |
| -------------------- | ---------------------------------- |
| `grok-*`, `gpt-*`    | ChatGPT-compatible Responses       |
| `minimax-*`, `qwen*` | Anthropic-compatible Messages      |
| All other ids        | OpenAI-compatible chat completions |

The base URL is `https://opencode.ai/zen/go/v1`. The selected endpoint determines
the request conversion and SSE stream parser.
