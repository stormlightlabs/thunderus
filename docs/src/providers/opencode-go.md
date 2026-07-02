# OpenCode Go Provider

OpenCode Go is selected with model ids in the `opencode-go/<model-id>` form,
for example:

```toml
model = "opencode-go/kimi-k2.7-code"
```

Set `OPENCODE_GO_KEY` in the environment or workspace `.env` file. API keys are
not accepted through CLI flags.

OpenCode Go exposes two endpoint families. `thndrs` routes Kimi, GLM,
DeepSeek, and MiMo models through the OpenAI-compatible chat-completions route,
and MiniMax/Qwen models through the Anthropic-compatible messages route.
