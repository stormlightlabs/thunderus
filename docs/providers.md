# Provider Specification

Thunderus targets three wire protocols. Every LLM provider maps to one of these.

## Protocol Summary

| Protocol             | Endpoint                                      | Auth                                                | Content Format                   |
| -------------------- | --------------------------------------------- | --------------------------------------------------- | -------------------------------- |
| OpenAI Completions   | `POST /v1/chat/completions`                   | `Authorization: Bearer {key}`                       | `messages[]` with roles          |
| OpenAI Responses     | `POST /v1/responses`                          | `Authorization: Bearer {key}`                       | `input` items + `instructions`   |
| Anthropic Messages   | `POST /v1/messages`                           | `x-api-key: {key}`, `anthropic-version: 2023-06-01` | `messages[]` with content blocks |
| Google Generative AI | `POST /v1beta/models/{model}:generateContent` | `?key={key}` query param                            | `contents[]` with parts          |

## 1. OpenAI Chat Completions

**Base URL:** `https://api.openai.com`

### Request

```json
{
  "model": "string",                          // required
  "messages": [                               // required
    {
      "role": "system" | "user" | "assistant" | "tool",
      "content": "string" | [ContentPart],
      "tool_calls": [ToolCall],               // assistant only
      "tool_call_id": "string"                // tool only
    }
  ],
  "temperature": 0.0–2.0,                    // default 1
  "max_tokens": int,                          // alias: max_completion_tokens
  "top_p": float,
  "stop": "string" | ["string"],              // up to 4
  "stream": bool,
  "stream_options": { "include_usage": true },
  "tools": [Tool],
  "tool_choice": "none" | "auto" | "required" | { "type": "function", "function": { "name": "..." } },
  "response_format": { "type": "text" | "json_object" | "json_schema" },
  "reasoning_effort": "low" | "medium" | "high"  // o-series only
}
```

**ContentPart:**

```json
{ "type": "text", "text": "..." }
{ "type": "image_url", "image_url": { "url": "...", "detail": "auto" | "low" | "high" } }
```

**Tool:**

```json
{
  "type": "function",
  "function": {
    "name": "string",
    "description": "string",
    "parameters": { /* JSON Schema */ },
    "strict": bool
  }
}
```

### Response

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "model": "string",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "string | null",
      "tool_calls": [{
        "id": "call_...",
        "type": "function",
        "function": { "name": "string", "arguments": "string" }  // JSON-encoded
      }]
    },
    "finish_reason": "stop" | "length" | "tool_calls" | "content_filter"
  }],
  "usage": {
    "prompt_tokens": int,
    "completion_tokens": int,
    "total_tokens": int
  }
}
```

### Streaming

SSE with `data: {json}` lines, terminated by `data: [DONE]`.

Chunk shape: same as response but `object: "chat.completion.chunk"`, `choices[].delta` instead of `choices[].message`. Fields appear incrementally: `role` in first chunk, `content`/`tool_calls` in subsequent chunks, `finish_reason` in final chunk. Usage in final chunk if `stream_options.include_usage` is set.

## 2. OpenAI Responses

**Base URL:** `https://api.openai.com`

### Request

```json
{
  "model": "string",                          // required
  "input": "string" | [InputItem],            // required
  "instructions": "string",                   // system prompt
  "temperature": 0.0–2.0,
  "max_output_tokens": int,
  "top_p": float,
  "stream": bool,
  "previous_response_id": "string",           // server-side multi-turn
  "tools": [Tool],
  "tool_choice": "none" | "auto" | "required" | { "type": "function", "name": "..." },
  "truncation": "disabled" | "auto",
  "reasoning": { "effort": "low" | "medium" | "high" },
  "text": { "format": { "type": "text" | "json_object" | "json_schema" } }
}
```

**InputItem:**

```json
{ "type": "message", "role": "user" | "assistant" | "system", "content": "string" | [ContentPart] }
{ "type": "item_reference", "id": "string" }
// Tool result (sent in next turn):
{ "type": "function_call_output", "call_id": "call_...", "output": "string" }
```

**Tool** (same as Completions, plus built-ins):

```json
{ "type": "web_search_preview" }
{ "type": "file_search", "vector_store_ids": ["vs_..."] }
{ "type": "code_interpreter" }
```

### Response

```json
{
  "id": "resp_...",
  "object": "response",
  "status": "completed" | "incomplete" | "failed",
  "output": [OutputItem],
  "usage": {
    "input_tokens": int,
    "output_tokens": int,
    "total_tokens": int
  },
  "error": null | { "code": "string", "message": "string" }
}
```

**OutputItem types:**

```json
{ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "..." }] }
{ "type": "function_call", "id": "fc_...", "call_id": "call_...", "name": "string", "arguments": "string" }
{ "type": "reasoning", "summary": [{ "type": "summary_text", "text": "..." }] }
```

### Streaming

SSE with named `event:` lines. Key events:

| Event                                    | Payload                                                            |
| ---------------------------------------- | ------------------------------------------------------------------ |
| `response.created`                       | Full response object, `status: "in_progress"`                      |
| `response.output_text.delta`             | `{ "delta": "string", "output_index": int, "content_index": int }` |
| `response.function_call_arguments.delta` | `{ "delta": "string", "output_index": int }`                       |
| `response.output_item.done`              | Finalized output item                                              |
| `response.completed`                     | Full response object, `status: "completed"`                        |

## 3. Anthropic Messages

**Base URL:** `https://api.anthropic.com`

### Request

```json
{
  "model": "string",                          // required
  "max_tokens": int,                          // required
  "messages": [                               // required, must alternate user/assistant
    {
      "role": "user" | "assistant",
      "content": "string" | [ContentBlock]
    }
  ],
  "system": "string" | [SystemBlock],
  "temperature": 0.0–1.0,                    // default 1
  "top_p": float,
  "top_k": int,
  "stop_sequences": ["string"],
  "stream": bool,
  "tools": [Tool],
  "tool_choice": { "type": "auto" } | { "type": "any" } | { "type": "tool", "name": "..." }
}
```

**ContentBlock types:**

```json
{ "type": "text", "text": "..." }
{ "type": "image", "source": { "type": "base64", "media_type": "image/jpeg", "data": "..." } }
{ "type": "tool_use", "id": "toolu_...", "name": "string", "input": {} }           // in assistant msgs
{ "type": "tool_result", "tool_use_id": "toolu_...", "content": "string", "is_error": bool }  // in user msgs
```

**Tool:**

```json
{
  "name": "string",
  "description": "string",
  "input_schema": {
    /* JSON Schema */
  }
}
```

### Response

```json
{
  "id": "msg_...",
  "type": "message",
  "role": "assistant",
  "content": [
    { "type": "text", "text": "..." },
    { "type": "tool_use", "id": "toolu_...", "name": "string", "input": {} }
  ],
  "model": "string",
  "stop_reason": "end_turn" | "max_tokens" | "stop_sequence" | "tool_use",
  "usage": {
    "input_tokens": int,
    "output_tokens": int
  }
}
```

### Streaming

SSE with `event:` + `data:` lines. Event sequence:

| Event                 | Data                                                                                          |
| --------------------- | --------------------------------------------------------------------------------------------- |
| `message_start`       | `{ "message": { ...partial response, usage with input_tokens... } }`                          |
| `content_block_start` | `{ "index": int, "content_block": { "type": "text", "text": "" } }`                           |
| `content_block_delta` | `{ "index": int, "delta": { "type": "text_delta", "text": "..." } }`                          |
| `content_block_delta` | `{ "index": int, "delta": { "type": "input_json_delta", "partial_json": "..." } }` (tool use) |
| `content_block_stop`  | `{ "index": int }`                                                                            |
| `message_delta`       | `{ "delta": { "stop_reason": "..." }, "usage": { "output_tokens": int } }`                    |
| `message_stop`        | `{}`                                                                                          |

Accumulate `input_json_delta` strings per block index; parse JSON only at `content_block_stop`.

## 4. Google Generative AI (Gemini)

**Base URL:** `https://generativelanguage.googleapis.com`

Streaming endpoint: `POST /v1beta/models/{model}:streamGenerateContent?key={key}&alt=sse`

### Request

```json
{
  "contents": [                               // required, alternating user/model
    {
      "role": "user" | "model",
      "parts": [Part]
    }
  ],
  "systemInstruction": {                      // top-level, not in contents
    "parts": [{ "text": "..." }]
  },
  "generationConfig": {
    "temperature": 0.0–2.0,
    "maxOutputTokens": int,
    "topP": float,
    "topK": int,
    "stopSequences": ["string"],
    "responseMimeType": "text/plain" | "application/json",
    "responseSchema": { /* JSON Schema */ },
    "thinkingConfig": { "includeThoughts": bool, "thinkingBudget": int }
  },
  "tools": [Tool],
  "toolConfig": {
    "functionCallingConfig": {
      "mode": "AUTO" | "ANY" | "NONE",
      "allowedFunctionNames": ["string"]      // only with ANY
    }
  },
  "safetySettings": [{ "category": "HARM_CATEGORY_...", "threshold": "BLOCK_..." }]
}
```

**Part types:**

```json
{ "text": "..." }
{ "inlineData": { "mimeType": "string", "data": "base64..." } }
{ "functionCall": { "name": "string", "args": {} } }            // model output
{ "functionResponse": { "name": "string", "response": {} } }    // user sends back
```

**Tool:**

```json
{
  "functionDeclarations": [
    {
      "name": "string",
      "description": "string",
      "parameters": { "type": "OBJECT", "properties": {}, "required": [] }
    }
  ]
}
```

Note: Google uses uppercase type names (`STRING`, `NUMBER`, `INTEGER`, `BOOLEAN`, `ARRAY`, `OBJECT`).

### Response

```json
{
  "candidates": [{
    "content": {
      "role": "model",
      "parts": [
        { "text": "..." },
        { "functionCall": { "name": "...", "args": {} } }
      ]
    },
    "finishReason": "STOP" | "MAX_TOKENS" | "SAFETY" | "RECITATION" | "MALFORMED_FUNCTION_CALL",
    "index": 0,
    "safetyRatings": [{ "category": "string", "probability": "string" }]
  }],
  "usageMetadata": {
    "promptTokenCount": int,
    "candidatesTokenCount": int,
    "totalTokenCount": int
  },
  "promptFeedback": { "blockReason": "string" }  // present if input was blocked
}
```

### Streaming

With `alt=sse`: SSE stream of `data: {json}` lines, each a complete `GenerateContentResponse`. Concatenate `candidates[0].content.parts[N].text` across chunks. Final chunk has authoritative `usageMetadata` and `finishReason`.

## Cross-Provider Mapping

This table maps equivalent concepts across protocols. Use this to design the internal abstraction.

| Concept                  | OpenAI Completions                     | OpenAI Responses                           | Anthropic                             | Google                               |
| ------------------------ | -------------------------------------- | ------------------------------------------ | ------------------------------------- | ------------------------------------ |
| **System prompt**        | `messages[0].role="system"`            | `instructions`                             | `system`                              | `systemInstruction`                  |
| **User message**         | `role: "user"`                         | `type: "message", role: "user"`            | `role: "user"`                        | `role: "user"`                       |
| **Assistant message**    | `role: "assistant"`                    | `type: "message", role: "assistant"`       | `role: "assistant"`                   | `role: "model"`                      |
| **Max output tokens**    | `max_tokens` / `max_completion_tokens` | `max_output_tokens`                        | `max_tokens`                          | `generationConfig.maxOutputTokens`   |
| **Tool definition**      | `tools[].function`                     | `tools[].function`                         | `tools[]` (flat)                      | `tools[].functionDeclarations[]`     |
| **Tool call (output)**   | `message.tool_calls[]`                 | `output[type=function_call]`               | `content[type=tool_use]`              | `parts[].functionCall`               |
| **Tool result (input)**  | `role: "tool"` + `tool_call_id`        | `type: "function_call_output"` + `call_id` | `type: "tool_result"` + `tool_use_id` | `parts[].functionResponse`           |
| **Tool call ID**         | `tool_calls[].id` / `tool_call_id`     | `call_id`                                  | `tool_use.id` / `tool_use_id`         | _(matched by name)_                  |
| **Stop reason: normal**  | `"stop"`                               | `status: "completed"`                      | `"end_turn"`                          | `"STOP"`                             |
| **Stop reason: length**  | `"length"`                             | `status: "incomplete"`                     | `"max_tokens"`                        | `"MAX_TOKENS"`                       |
| **Stop reason: tool**    | `"tool_calls"`                         | _(always completed)_                       | `"tool_use"`                          | _(check for functionCall in parts)_  |
| **Input tokens**         | `usage.prompt_tokens`                  | `usage.input_tokens`                       | `usage.input_tokens`                  | `usageMetadata.promptTokenCount`     |
| **Output tokens**        | `usage.completion_tokens`              | `usage.output_tokens`                      | `usage.output_tokens`                 | `usageMetadata.candidatesTokenCount` |
| **Streaming text delta** | `choices[].delta.content`              | `response.output_text.delta`               | `text_delta` in `content_block_delta` | `candidates[].content.parts[].text`  |
| **Stream termination**   | `data: [DONE]`                         | `response.completed` event                 | `message_stop` event                  | `data: [DONE]` (with `alt=sse`)      |

## Provider → Protocol Mapping

| Provider        | Protocol                 | Base URL                                    | Notes                               |
| --------------- | ------------------------ | ------------------------------------------- | ----------------------------------- |
| OpenAI          | Completions or Responses | `https://api.openai.com`                    |                                     |
| Anthropic       | Messages                 | `https://api.anthropic.com`                 | Requires `anthropic-version` header |
| Google          | Generative AI            | `https://generativelanguage.googleapis.com` | Model in URL path                   |
| Moonshot (Kimi) | OpenAI Completions       | `https://api.moonshot.ai`                   | Drop-in compatible                  |
| Zhipu (GLM)     | OpenAI Completions       | `https://api.z.ai/api/coding/paas`          | Coding Plan billing tier            |

## Implementation Notes

1. **Message alternation.** Anthropic and Google require strict user/assistant alternation. OpenAI does not. Normalize on the strict model; merge consecutive same-role messages before sending.

2. **Tool call ID matching.** Google matches tool calls by function name, not by ID. All other providers use explicit IDs. The abstraction must generate and track IDs internally for Google.

3. **Tool arguments encoding.** OpenAI returns tool arguments as a JSON-encoded string. Anthropic and Google return them as a parsed object. Normalize to parsed object internally.

4. **Streaming reassembly.** Each protocol has different delta granularity. The internal stream interface should emit a uniform `(event_type, index, data)` tuple where `event_type` is one of: `message_start`, `text_delta`, `tool_call_start`, `tool_call_delta`, `tool_call_end`, `message_end`.

5. **System prompt placement.** Each provider puts it somewhere different. The abstraction takes a `system: String` and maps it to the right location per protocol.

6. **`max_tokens` is required** for Anthropic but optional everywhere else. Always set it explicitly.
