---
title: "Umans Provider Notes"
---

Sources:

- Umans code docs: https://app.umans.ai/offers/code/docs#api-reference
- Umans model metadata: https://api.code.umans.ai/v1/models/info
- Lectito local repo: `~/Projects/StormlightLabs/OpenSource/lectito`

Reviewed on: 2026-06-28.

## Thesis

Umans is useful as a provider-design reference because it combines a
standard-shaped Messages API, multiple coding model routes, reasoning streams,
tool calling, model metadata, and server-side web search. Those features make it
a good case study for keeping provider-specific HTTP details behind normalized
agent events and tool records.

## Reasoning Streams

The docs describe reasoning output differently by route:

- `/v1/messages`: reasoning appears as `thinking` content blocks and streams as
  `thinking_delta` events.
- `/v1/chat/completions`: reasoning appears as `reasoning_content` on messages
  and streamed deltas.

Provider-design implication: keep reasoning as a first-class event distinct from
assistant answer text. The transcript can render it as secondary status or
collapsible context, but app state should not concatenate it blindly into the
final assistant response.

## Lectito Reuse

Repo: `github:stormlightlabs/lectito`.

Useful existing pieces:

- `lectito` crate exports `extract`, `extract_with_diagnostics`,
  `clean_article_html`, `html_to_markdown`, `is_probably_readable`, and related
  article/config types.
- The core crate does not fetch pages; callers pass HTML plus an optional base
  URL.
- `lectito-mcp` includes a `DuckDuckGoSearch` helper against `https://html.duckduckgo.com/html/`.
  - DuckDuckGo parsing returns title, normalized URL, and snippet.
  - The parser detects common bot-challenge pages before returning results.
- `lectito-mcp` exposes `search_articles` and `read_article` tools.
- `read_article` fetches public `http`/`https` URLs, follows bounded redirects,
  rejects private-network targets by default, checks HTML content type, limits
  response size, extracts with `extract_with_diagnostics`, and chunks output.

Reusable provider/search split:

- Server-side search: provider-native search when the selected provider supports
  it and the user/search policy allows it.
- Local fallback: bounded DuckDuckGo-style HTML search when server-side search
  is disabled or unavailable.
- Extraction: deterministic readable Markdown/text extraction from pages
  selected by search or supplied by the user.

Keep this surface bounded: no browser, crawler, or full MCP stack unless a
concrete product need outweighs the extra state, security, and debugging cost.

## Open Questions

- How should provider-native search, reasoning streams, and local extraction be separated?
  - **Recommendation**: Normalize provider reasoning and search into explicit events while keeping local extraction as a bounded fallback rather than a browser or crawler layer.
