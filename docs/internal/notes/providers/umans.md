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

## API Shape

Manual configuration in the docs lists:

- Base URL: `https://api.code.umans.ai`
- Anthropic endpoint: `https://api.code.umans.ai/v1/messages`
- OpenAI endpoint: `https://api.code.umans.ai/v1/chat/completions`
- Default model name: `umans-coder`

Anthropic-compatible request example:

- `POST /v1/messages`
- `Content-Type: application/json`
- `x-api-key: sk-your-umans-api-key`
- `anthropic-version: 2023-06-01`
- body includes `model`, `messages`, `max_tokens`, and `stream`

OpenAI-compatible request example:

- `POST /v1/chat/completions`
- `Authorization: Bearer sk-your-umans-api-key`
- body includes `model`, `messages`, and `stream`

Recommendation: start with `/v1/messages`. It is the most direct fit for a
Claude Code/Pi-style coding harness, and Umans notes that GLM 5.2's composite
vision path only works on this route.

## Target Models

### `umans-coder`

Current metadata:

- Display name: Umans Coder
- Base model: Kimi K2.7-Code by Moonshot
- Role: recommended model for complex coding-heavy workloads
- Context window: 262,144 tokens
- Recommended max output: 32,768 tokens
- Max completion tokens: 262,144
- Supports tools: yes
- Supports vision: yes
- Reasoning: supported, cannot be disabled

Use this as the default model.

### `umans-glm-5.2`

Current metadata:

- Display name: Umans GLM 5.2
- Base model: GLM-5.2
- Role: latest GLM option with the largest context window
- Context window: 405,504 tokens
- Recommended max output: 131,071 tokens
- Max completion tokens: 131,072
- Supports tools: yes
- Supports vision: via server-side handoff on `/v1/messages`
- Reasoning: supported, can be disabled
- Reasoning levels: `none`, `high`, `max`
- Default reasoning level: `high`

Use this as an explicit alternate/deep-context model. Do not make it the hidden
default.

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

## Native Web Search

The docs expose a server-side web-search selector:

- `native`: Umans web search, Kimi-backed path
- `exa`: Umans web search, Exa-backed path
- `none`: disable server-side search and pass the caller's own `web_search`
  tool through unchanged

CLI examples use:

- `umans claude --websearch native`
- `umans claude --websearch exa`
- `umans claude --websearch none`

Direct API calls can set:

- `X-Umans-Websearch-Provider: native`
- `X-Umans-Websearch-Provider: exa`
- `X-Umans-Websearch-Provider: none`

The override only matters when the request carries a web-search tool and when
Umans owns the search step. If neither CLI nor header sets a backend, Umans uses
its default backend.

Recommendation: default `thndrs` to `native`, expose `exa` and `none` in config,
and render server-side search activity through normal tool transcript entries.

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

## Implementation Implications

- Start with a concrete `UmansClient`, not a `Provider` trait.
- Read `UMANS_API_KEY` from the environment.
- Use `umans-coder` by default and make `umans-glm-5.2` selectable.
- Fetch `/v1/models/info` for visible model capability metadata.
- Normalize all provider stream output into `AgentEvent`.
- Add an `AgentEvent` variant for reasoning/thinking deltas before wiring real
  streams.
- Model search as tool events whether Umans executes it server-side or Lectito
  executes it locally.
- Add no-network tests around request construction and stream fixture parsing
  before any live-provider smoke test.
