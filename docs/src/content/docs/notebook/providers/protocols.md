---
title: "Provider Protocol Mapping"
sources:
  - https://platform.openai.com/docs/api-reference/responses
  - https://platform.claude.com/docs/en/api/messages
  - https://ai.google.dev/gemini-api/docs/function-calling
Author: OpenAI, Anthropic, Google
Date: captured 2026-07-04
Tags: [providers, streaming, tools, protocol, normalization]
---

## Summary

Provider APIs expose similar agent concepts through different wire shapes, so a
coding harness needs provider-specific adapters that normalize messages,
reasoning, tool calls, tool results, streaming, and usage.

## Key Ideas

- **System context placement varies:** OpenAI Responses uses `instructions`,
  Claude Messages uses a top-level system field, and Gemini uses
  `systemInstruction`.
- **Tool-call representation varies:** OpenAI, Claude, and Gemini expose tool
  definitions and tool calls through different JSON structures.
- **Streaming is not uniform:** Text deltas, tool argument deltas, completion
  events, and usage metadata have provider-specific event names and shapes.
- **Usage belongs in neutral records:** Provider adapters should map usage to a
  stable internal shape before sessions or UI consume it.
- **Provider-native tools complicate policy:** Web search, file search, MCP, and
  function tools need explicit policy about whether they are provider-native or
  harness-owned.

## Claims & Evidence

### OpenAI Responses is a tool-capable response interface.

The Responses API supports input items, instructions, streaming, built-in tools,
custom function calling, previous response state, and conversations.

Confidence: high.

### Claude Messages uses content blocks.

Claude Messages represents user and assistant content as blocks, including tool
use and tool result blocks.

Confidence: high.

### Gemini uses function declarations and function responses.

Gemini function calling declares callable functions and exchanges function calls
and responses through model/user parts.

Confidence: high.

## Important Terms

| Term           | Meaning                                                              |
| -------------- | -------------------------------------------------------------------- |
| Adapter        | Provider-specific code that maps wire protocol into internal events. |
| Tool call      | Model request to invoke a named function/tool.                       |
| Tool result    | Harness response sent back to satisfy a tool call.                   |
| Usage          | Provider-reported token accounting.                                  |
| Semantic event | Internal provider-neutral event.                                     |

## Questions for Review

- What is the minimal provider-neutral stream event enum?
- Which raw provider metadata should be retained in verbose logs?
- How should provider-native web search be represented in sessions?
- How should providers without stable tool IDs be normalized?

## Connections

- Related ideas: tool registry, session export, provider retry, web search,
  reasoning display.
- Related sources: Umans provider notes, OpenCode Go provider notes.
- Useful applications: roadmap provider normalization refactor.

## Open Questions

- Should provider-native MCP support be exposed, or should MCP remain
  harness-owned?
- Should Responses API concepts influence internal turn-event naming?
- How should provider-specific safety/filter stops appear in transcript rows?

## Takeaways

- Normalize at the provider boundary.
- Keep reasoning, assistant text, tools, and usage separate.
- Treat provider-native tools as policy decisions, not automatic defaults.
