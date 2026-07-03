---
title: "OpenCode Go"
Source: https://opencode.ai/docs/go
Author: OpenCode / Anomaly
Date: 2026-07-01
Captured: 2026-07-01
Tags: providers, llm-api, opencode, open-models
---

## Summary

OpenCode Go is a paid OpenCode provider that curates open coding models behind OpenAI-compatible
and Anthropic-compatible API endpoints, with model IDs, usage limits, and metadata exposed
for direct API use.

## Key Ideas

- **Provider shape:** Go behaves like any other OpenCode provider: users subscribe, configure
  an API key, and then select Go models from the provider's model list.
- **Curated open-model access:** The offering is based on tested model/provider combinations
  rather than a raw pass-through to every available open model.
- **Two API families:** Go exposes some models through an OpenAI-compatible chat completions
  route and others through an Anthropic-compatible messages route.
- **OpenCode model IDs:** In OpenCode config, Go model IDs use the `opencode-go/<model-id>`
  form, for example `opencode-go/kimi-k2.7-code`.
- **Programmatic model discovery:** The full model list and metadata are available from
  `https://opencode.ai/zen/go/v1/models`.
- **Usage metering:** Limits are defined in dollar value over 5-hour, weekly, and monthly
  windows, so effective request counts depend on each model's token prices and request shape.

## Claims & Evidence

| Claim                                                                                | Support                                                                                                                                      | Caveat / Confidence                                               |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Go is optional and works as a normal OpenCode provider.                              | Docs describe subscribing to OpenCode Go, copying an API key, connecting via `/connect`, and using `/models` to see available models.        | High. This is direct product documentation.                       |
| Go curates open coding models rather than exposing an arbitrary model catalog.       | Background section says OpenCode tested selected open models, worked with providers, benchmarked combinations, and recommends a chosen list. | High.                                                             |
| API consumers must handle both OpenAI-compatible and Anthropic-compatible endpoints. | Endpoint table maps GLM, Kimi, DeepSeek, and MiMo models to `/v1/chat/completions`, while MiniMax and Qwen models use `/v1/messages`.        | High. Exact mapping may change as models are added or removed.    |
| Model IDs are stable enough to configure directly.                                   | The docs list model names and model IDs and state that OpenCode config uses `opencode-go/<model-id>`.                                        | Medium-high. Docs also say the list may change.                   |
| Usage limits are value-based, not request-count based.                               | The docs list $12 / 5-hour, $30 / week, and $60 / month limits and explain request count depends on the selected model.                      | High for current docs; plan limits may change.                    |
| Users can continue beyond plan limits with Zen balance.                              | Docs say enabling "Use balance" falls back to Zen balance after Go limits are reached.                                                       | High.                                                             |
| Providers follow zero-retention and no-training policies.                            | Privacy section says models are hosted in US, EU, and Singapore and providers follow zero-retention and do not train on user data.           | Medium. This is a stated policy, not independently verified here. |

## Important Terms

| Term                          | Meaning                                                                                                     |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------- |
| OpenCode Go                   | OpenCode's subscription provider for curated open coding models.                                            |
| OpenCode Zen                  | The account/billing surface where users subscribe to Go and get the API key.                                |
| `opencode-go/<model-id>`      | Model identifier format used in OpenCode configuration for Go-backed models.                                |
| `/zen/go/v1/chat/completions` | Go endpoint for OpenAI-compatible chat completions models.                                                  |
| `/zen/go/v1/messages`         | Go endpoint for Anthropic-compatible messages models.                                                       |
| `/zen/go/v1/models`           | Model discovery endpoint for the complete Go model list and metadata.                                       |
| Cached read                   | Pricing category for tokens read from provider-side prompt/cache state.                                     |
| Cached write                  | Pricing category for tokens written into provider-side cache, present for some Anthropic-compatible routes. |

## Model And Endpoint Map

| Model             | Model ID            | Endpoint                                         | AI SDK package              |
| ----------------- | ------------------- | ------------------------------------------------ | --------------------------- |
| GLM-5.2           | `glm-5.2`           | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| GLM-5.1           | `glm-5.1`           | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| Kimi K2.7 Code    | `kimi-k2.7-code`    | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| Kimi K2.6         | `kimi-k2.6`         | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| DeepSeek V4 Pro   | `deepseek-v4-pro`   | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| DeepSeek V4 Flash | `deepseek-v4-flash` | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| MiMo-V2.5         | `mimo-v2.5`         | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| MiMo-V2.5-Pro     | `mimo-v2.5-pro`     | `https://opencode.ai/zen/go/v1/chat/completions` | `@ai-sdk/openai-compatible` |
| MiniMax M3        | `minimax-m3`        | `https://opencode.ai/zen/go/v1/messages`         | `@ai-sdk/anthropic`         |
| MiniMax M2.7      | `minimax-m2.7`      | `https://opencode.ai/zen/go/v1/messages`         | `@ai-sdk/anthropic`         |
| MiniMax M2.5      | `minimax-m2.5`      | `https://opencode.ai/zen/go/v1/messages`         | `@ai-sdk/anthropic`         |
| Qwen3.7 Max       | `qwen3.7-max`       | `https://opencode.ai/zen/go/v1/messages`         | `@ai-sdk/anthropic`         |
| Qwen3.7 Plus      | `qwen3.7-plus`      | `https://opencode.ai/zen/go/v1/messages`         | `@ai-sdk/anthropic`         |
| Qwen3.6 Plus      | `qwen3.6-plus`      | `https://opencode.ai/zen/go/v1/messages`         | `@ai-sdk/anthropic`         |

## API Notes

- The docs present Go as directly API-accessible, not only usable through the OpenCode TUI.
- Model discovery endpoint: `GET https://opencode.ai/zen/go/v1/models`.
- OpenAI-compatible models use `POST https://opencode.ai/zen/go/v1/chat/completions`.
- Anthropic-compatible models use `POST https://opencode.ai/zen/go/v1/messages`.
- The docs identify AI SDK adapters by endpoint family: `@ai-sdk/openai-compatible` for chat
  completions and `@ai-sdk/anthropic` for messages.
- The page does not show authentication headers for direct API calls; it only says users
  obtain an API key through OpenCode Zen.

## Usage And Pricing Concepts

- Limits are value budgets: $12 per 5-hour window, $30 weekly, and $60 monthly.
- Estimated request counts are derived from observed request shapes, including input tokens,
  cached tokens, and output tokens.
- Expensive models such as GLM-5.2 and Qwen3.7 Max produce fewer estimated requests per
  budget window.
- Cheaper models such as DeepSeek V4 Flash and MiMo-V2.5 produce much higher estimated
  request counts.
- Pricing is per 1M tokens and may include input, output, cached read, and cached write
  categories depending on the model/provider route.
- If a user has Zen balance and enables balance use, Go can fall back to balance after
  subscription limits are reached.

## Questions for Review

- What are the two endpoint families OpenCode Go exposes?
  - Model the provider as two endpoint families, OpenAI-compatible chat completions and Anthropic-compatible messages.
- How does the OpenCode config model ID differ from the raw model ID?
  - Keep config-facing ids namespaced with `opencode-go/` and translate to raw provider ids at request time.
- Why do value-based usage limits produce different request counts by model?
  - Present value limits with estimated token-cost impact rather than implying a fixed request count.
- Which Go models use the Anthropic-compatible messages endpoint?
  - Discover endpoint family from model metadata instead of hardcoding a permanent model list.
- What endpoint should be queried to discover the current model list and metadata?
  - Query `/zen/go/v1/models` for current model metadata before building picker or routing behavior.
- What authentication details are missing from the Go docs page?
  - Verify header names, token format, and error bodies empirically before documenting direct API usage.

## Connections

- Related ideas: OpenAI-compatible provider adapters, Anthropic-compatible provider adapters,
  model metadata discovery, cached-token pricing.
- Related sources: Umans Code provider docs, OpenCode provider/model docs, AI SDK provider
  adapters.
- Contradictions or tensions: The provider aims to avoid lock-in, but direct API usage
  still depends on OpenCode Zen subscription and API-key semantics.
- Conceptual uses: implementing a Go provider requires endpoint-family
  selection per model, model discovery, value-limit/error handling, and a
  config-facing `opencode-go/<model-id>` naming convention.

## Open Questions

- What exact auth header does direct API usage require: `Authorization: Bearer`,
  provider-specific header, or another OpenCode Zen convention?
  - **Recommendation**: Verify authentication against live docs or a real request before
    implementing a provider client.
- What response schema does `GET /zen/go/v1/models` return, and does it include endpoint
  family, pricing, context window, tool support, and reasoning support?
  - **Recommendation**: Treat OpenCode Go as a dynamic provider that needs model discovery
    and per-model endpoint routing before hardcoding request behavior.
- Are streaming responses supported on both endpoint families, and do they exactly match
  upstream OpenAI/Anthropic event shapes?
  - **Recommendation**: Implement streaming only after fixture captures prove each endpoint
    family's event shape and termination semantics.
- How are rate-limit and value-limit errors represented in HTTP status and JSON body?
  - **Recommendation**: Capture and normalize limit errors separately from transport
    failures so users can distinguish quota exhaustion from broken requests.
- Do any Go models support tool calling, reasoning content, prompt caching controls, image
  input, or server-side web search?
  - **Recommendation**: Derive capabilities from model metadata when available and avoid
    assuming feature parity across Go models.
- Are regional hosting choices automatic, account-scoped, or model/provider-specific?
  - **Recommendation**: Treat region behavior as opaque unless the provider exposes
    explicit routing controls or compliance guarantees.

## Notable Quotes

> "The list of models may change as we test and add new ones."

## Takeaways

- OpenCode Go is a curated open-model provider with direct API endpoints.
- Provider implementation needs per-model routing between OpenAI-compatible and
  Anthropic-compatible APIs.
- The model discovery endpoint and missing auth/error/streaming details are the
  highest-value follow-up research items.
