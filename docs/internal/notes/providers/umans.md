# Umans Provider Notes

Sources:

- Umans code docs: https://app.umans.ai/offers/code/docs#api-reference
- Umans model metadata: https://api.code.umans.ai/v1/models/info
- Lectito local repo: `~/Projects/StormlightLabs/OpenSource/lectito`

Reviewed on: 2026-06-28.

## Thesis

Umans should be the first provider for `thndrs`. It gives us the two target
models out of the gate, works through standard Anthropic/OpenAI-compatible API
shapes, and includes native server-side web search. That lets the harness focus
on state, streaming, UI, and tool-event rendering before it grows a generic
provider system.

## Reasoning Streams

The docs describe reasoning output differently by route:

- `/v1/messages`: reasoning appears as `thinking` content blocks and streams as
  `thinking_delta` events.
- `/v1/chat/completions`: reasoning appears as `reasoning_content` on messages
  and streamed deltas.

Implication for `thndrs`: keep reasoning as a first-class event distinct from
assistant answer text. The transcript can render it as collapsible/secondary
status later, but the app state should not concatenate it blindly into the final
assistant response.

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

Recommended role in `thndrs`:

- Primary search: Umans native server-side search.
- Local fallback: Lectito-style DuckDuckGo search when Umans search is disabled or unavailable.
- Extraction: Lectito core for deterministic readable Markdown/text from pages
  selected by local search or user-provided URLs.

Keep this read-only at first.

Do not add a browser, crawler, or full MCP stack until the concrete harness needs it.
