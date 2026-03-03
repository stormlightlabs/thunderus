# Model Specification

Initially supported providers and their model configurations. Both providers use the OpenAI Chat Completions wire protocol with provider-specific quirks documented below.

## Provider Overview

| Provider        | Base URL                               | Auth                              | Protocol Quirks                                                                       |
| --------------- | -------------------------------------- | --------------------------------- | ------------------------------------------------------------------------------------- |
| Moonshot (Kimi) | `https://api.moonshot.ai/v1`           | `Authorization: Bearer sk-...`    | `temperature` 0.0–1.0, `n=1` only, no `logprobs`, thinking mode via response field    |
| Zhipu (GLM)     | `https://api.z.ai/api/coding/paas/v4/` | `Authorization: Bearer {api_key}` | `temperature` 0.0–1.0, no `logprobs`/`logit_bias`, thinking mode via `thinking` param |

## 1. Moonshot Kimi Models

### Models

| Model ID    | Type                                    | Context | Max Output | Notes                                          |
| ----------- | --------------------------------------- | ------- | ---------- | ---------------------------------------------- |
| `kimi-k2.5` | Multimodal MoE (~1T params, 32B active) | 256K    | 65K        | Flagship. Visual coding, agentic tool calling. |

Kimi K2.5 (released Jan 2026) supersedes Kimi K2. Built on K2 with continued pretraining over ~15T mixed visual and text tokens. Supports four modes: **Instant**, **Thinking**, **Agent**, and **Agent Swarm**.

### Thinking Mode

Kimi K2.5 supports a thinking/reasoning mode that returns reasoning traces alongside the final answer. In thinking mode, responses include a `reasoning_content` field in addition to `content`.

- **Instant mode** (default): direct responses, no reasoning trace.
- **Thinking mode**: includes `reasoning_content` in `choices[].message` and `choices[].delta` during streaming.

The exact parameter to enable thinking mode should be verified against `https://platform.moonshot.ai/docs/api/chat`.

### Feature Support

| Feature                                  | Supported | Notes                                        |
| ---------------------------------------- | --------- | -------------------------------------------- |
| Streaming                                | Yes       | Standard SSE, `data: [DONE]` termination     |
| System messages                          | Yes       | `role: "system"`                             |
| Tool calling                             | Yes       | Standard OpenAI `tools` schema               |
| `tool_choice`                            | Yes       | `"none"`, `"auto"`, `"required"`             |
| Vision                                   | Yes       | `image_url` content parts (URL and base64)   |
| JSON mode                                | Yes       | `response_format: { "type": "json_object" }` |
| Thinking mode                            | Yes       | Returns `reasoning_content` in response      |
| `stream_options.include_usage`           | Yes       |                                              |
| `presence_penalty` / `frequency_penalty` | Accepted  | May be no-op depending on model              |
| `n > 1`                                  | No        | Only single completion                       |
| `logprobs`                               | No        |                                              |

### Deviations from OpenAI

1. **`temperature` range is 0.0–1.0.** Values above 1.0 will error. OpenAI allows 0.0–2.0.
2. **No `logprobs` or `logit_bias`.**
3. **No multi-completion** (`n` must be 1).
4. **Thinking mode** returns `reasoning_content` alongside `content` in `choices[].message` (non-streaming) and `choices[].delta` (streaming). Handle this extra field gracefully don't discard it, surface it in the UI.
5. **Prompt caching** is automatic on Kimi K2.5 cache hits are reflected in pricing, not in the response payload.
6. **API domain changed** from `api.moonshot.cn` to `api.moonshot.ai`. Both may work but `.ai` is the current primary.

## 2. Zhipu GLM Models (Coding Plan)

Thunderus uses Zhipu's **Coding Plan** tier, which provides access to GLM-4.x through GLM-5 at a code-optimized endpoint.

### Coding Plan Endpoint

```text
POST https://api.z.ai/api/coding/paas/v4/chat/completions
```

This is a separate billing tier from the standard BigModel API. The wire format is identical (OpenAI-compatible) but the base URL path includes `/coding/`.

### Models

| Model ID              | Type                            | Context | Max Output | Notes                                |
| --------------------- | ------------------------------- | ------- | ---------- | ------------------------------------ |
| `glm-5`               | Flagship MoE (745B, 44B active) | 200K    | 128K       | SOTA coding + agents. Thinking mode. |
| `glm-5-code`          | Code-specialized                | -       | -          | Higher coding-focused tier           |
| `glm-4.7`             | Code-optimized                  | 128K    | 4K–8K      | Available via Coding Plan            |
| `glm-4.7-flashx`      | Fast code tier                  | -       | -          | Lower-cost FlashX variant            |
| `glm-4.7-flash`       | Fast free tier                  | -       | -          | Free tier                            |
| `glm-4.6`             | Code-optimized                  | 128K    | 4K–8K      | Available via Coding Plan            |
| `glm-4.5`             | General                         | 128K    | 4K–8K      | Available via Coding Plan            |
| `glm-4.5-x`           | Premium                         | -       | -          | Higher quality/cost tier             |
| `glm-4.5-air`         | Lightweight                     | -       | -          | Cost-efficient tier                  |
| `glm-4.5-airx`        | Lightweight premium             | -       | -          | Mid-tier price/perf                  |
| `glm-4.5-flash`       | Fast                            | 128K    | 4K         | Available via Coding Plan            |
| `glm-4-32b-0414-128k` | 32B model                       | 128K    | -          | Legacy low-cost option               |

GLM-5 (released Feb 2026) is a 745B MoE model trained entirely on Huawei Ascend chips. 256 experts, 8 active per token.

### Thinking Mode

GLM-5 supports a thinking/reasoning mode enabled via a request parameter:

```json
{
  "model": "glm-5",
  "messages": [...],
  "thinking": { "type": "enabled" }
}
```

When enabled, the model includes reasoning traces in its response. The exact response field for thinking content should be verified against `https://docs.z.ai/guides/llm/glm-5`.

### Feature Support

| Feature               | Supported | Notes                                        |
| --------------------- | --------- | -------------------------------------------- |
| Streaming             | Yes       | Standard SSE, `data: [DONE]` termination     |
| System messages       | Yes       | `role: "system"`                             |
| Tool calling          | Yes       | Standard OpenAI `tools` schema               |
| `tool_choice`         | Yes       |                                              |
| Thinking mode         | Yes       | GLM-5: `thinking: {"type": "enabled"}`       |
| JSON mode             | Yes       | `response_format: { "type": "json_object" }` |
| Structured output     | Yes       | JSON schema support                          |
| Context caching       | Yes       |                                              |
| Web search (built-in) | Yes       | Native `web_search` tool type (see below)    |
| `n > 1`               | No        | Limited or unsupported                       |
| `logprobs`            | No        |                                              |
| `logit_bias`          | No        |                                              |

### Deviations from OpenAI

1. **`temperature` range is 0.0–1.0.** Same constraint as Moonshot.
2. **No `logprobs` or `logit_bias`.**
3. **Extra parameter: `do_sample`** (bool) - enables/disables sampling. Defaults to `true` when `temperature > 0`.
4. **Extra parameter: `request_id`** (string) - client-side idempotency/tracing ID.
5. **Native `web_search` tool type** - not present in OpenAI. See format below.
6. **Auth**: Supports both JWT (legacy) and plain Bearer token. Use Bearer for simplicity.

### Web Search Tool (Zhipu-specific)

```json
{
  "type": "web_search",
  "web_search": {
    "enable": true,
    "search_query": "optional override query"
  }
}
```

This is a non-standard tool type. When enabled, the model can ground responses with web results. This goes in the `tools` array alongside standard function tools.

### Auth Details

Zhipu API keys have the format `{id}.{secret}`. Two auth modes:

**Simple (recommended):**

```text
Authorization: Bearer {id}.{secret}
```

**JWT (legacy):**
Generate a JWT with `HS256` using the secret portion, include `api_key`, `exp`, and `timestamp` claims. Not needed for OpenAI SDK compatibility Bearer passthrough works.

The Coding Plan API at `https://api.z.ai/api/coding/paas/v4/` uses the same auth as the standard BigModel API.

## Cross-Model Comparison

| Capability     | Kimi K2.5                 | GLM-5                                 |
| -------------- | ------------------------- | ------------------------------------- |
| Context window | 256K                      | 200K                                  |
| Max output     | 65K                       | 128K                                  |
| Architecture   | MoE ~1T (32B active)      | MoE 745B (44B active)                 |
| Tool calling   | Strong (agentic focus)    | Strong (SOTA coding + agents)         |
| Vision         | Yes (native multimodal)   | Check model-specific support          |
| Thinking mode  | Yes (`reasoning_content`) | Yes (`thinking: {"type": "enabled"}`) |
| Web search     | No                        | Built-in tool                         |

## Implementation Constraints

Both providers use the OpenAI Completions protocol but share these constraints that differ from OpenAI proper:

1. **Clamp `temperature`** to `[0.0, 1.0]` before sending. Both providers reject values > 1.0.
2. **Strip unsupported fields** (`logprobs`, `logit_bias`, `n` > 1) from requests. Don't send them some providers return errors, others silently ignore.
3. **Handle Zhipu's `web_search` tool** if the internal tool list contains a web search tool targeting Zhipu, serialize it as `{ "type": "web_search", ... }` not `{ "type": "function", ... }`.
4. **Handle thinking mode per-provider** Moonshot returns `reasoning_content` as a response field. Zhipu uses `thinking: {"type": "enabled"}` as a request parameter. The abstraction must normalize thinking mode activation and response parsing.
5. **Output token limits vary significantly** Kimi K2.5 allows 65K output vs GLM-5 at 128K. Set `max_tokens` per-model, not globally.
6. **Zhipu Coding Plan endpoint** use `/api/coding/paas/v4/` not `/api/paas/v4/`. Same wire format, different billing tier.
