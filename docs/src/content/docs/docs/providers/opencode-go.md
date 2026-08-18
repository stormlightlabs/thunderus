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

## Reasoning controls

`Auto` is available for every model. The other choices depend on the selected
model and appear in the `/reasoning` picker (or the reasoning picker opened
after `/model`):

| Model | Additional choices |
| --- | --- |
| Grok 4.5 | `low`, `medium`, `high` |
| GLM-5.3 | `low`, `high`, `max` |
| GLM-5.2 | `high`, `max` |
| GPT 5.6 Luna | `none`, `low`, `medium`, `high`, `xhigh`, `max` |
| Kimi K3 | `max` |
| MiniMax M3 | `none`, `on` |
| Qwen3.8 Max, Qwen3.7 Max, Qwen3.7 Plus, Qwen3.6 Plus | `high`, `max` |
| DeepSeek V4 Pro | `high`, `max` |
| DeepSeek V4 Flash | `low`, `high`, `max` |
| Hy3 | `none`, `low`, `high` |
| GLM-5.1, Kimi K2.7 Code, Kimi K2.6, MiMo-V2.5, MiMo-V2.5-Pro, MiniMax M2.7, MiniMax M2.5 | `Auto` only |

The controls follow the model capability profiles used by OpenCode. OpenCode Go's
`/models` endpoint returns model ids without those profiles, so an unlisted live
model exposes `Auto` until its provider profile is added.

The selected endpoint receives the native request shape for its model family:

- Responses models receive `reasoning.effort`; `reasoning_summary = "auto"`
  also requests a provider reasoning summary.
- OpenAI-compatible chat models receive top-level `reasoning_effort`.
- Anthropic-compatible Messages models receive `thinking`. `none` disables
  thinking, `on` enables adaptive thinking for MiniMax M3, and `high`/`max`
  use enabled thinking budgets for Qwen models. `high` uses half of the request
  output budget; `max` uses the request output budget minus one token.

These fields are provider request details. The application-facing setting stays
`reasoning_effort` in configuration and the shared picker.
