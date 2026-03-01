# Tool Specification

Tools are functions the agent can invoke during a conversation. Each tool is exposed to the LLM via the provider's tool-calling protocol (see `providers.md` for wire format per provider).

## 1. Web Search - Brave Search API

### Why Brave

Privacy-respecting, independent index, generous free tier (2K queries/month), single API key auth, clean JSON responses. No Google/Bing dependency.

### API Reference

**Base URL:** `https://api.search.brave.com/res/v1`

**Auth:**

```text
X-Subscription-Token: {api_key}
Accept: application/json
```

### Endpoints

| Endpoint     | Method | Path                 | Free Tier             |
| ------------ | ------ | -------------------- | --------------------- |
| Web Search   | GET    | `/web/search`        | Yes                   |
| News Search  | GET    | `/news/search`       | No                    |
| Image Search | GET    | `/images/search`     | No                    |
| Video Search | GET    | `/videos/search`     | No                    |
| Summarizer   | GET    | `/summarizer/search` | No (Data for AI plan) |

We use **Web Search** only. It returns web results, news, discussions, FAQ, and infobox in a single call - the other endpoints are redundant for agent use.

### Request

```text
GET /res/v1/web/search?q={query}&count={count}&...
```

| Parameter          | Type   | Required | Default      | Description                                                                               |
| ------------------ | ------ | -------- | ------------ | ----------------------------------------------------------------------------------------- |
| `q`                | string | Yes      | -            | Search query. Max 400 chars.                                                              |
| `count`            | int    | No       | 20           | Results per page. 1–20.                                                                   |
| `offset`           | int    | No       | 0            | Page offset. 0–9.                                                                         |
| `country`          | string | No       | `"US"`       | ISO 3166-1 alpha-2                                                                        |
| `search_lang`      | string | No       | `"en"`       | BCP 47 language                                                                           |
| `safesearch`       | string | No       | `"moderate"` | `"off"` / `"moderate"` / `"strict"`                                                       |
| `freshness`        | string | No       | -            | `"pd"` (day), `"pw"` (week), `"pm"` (month), `"py"` (year), or `"YYYY-MM-DDtoYYYY-MM-DD"` |
| `result_filter`    | string | No       | -            | Comma-separated: `web`, `news`, `discussions`, `faq`, `infobox`, `videos`                 |
| `extra_snippets`   | bool   | No       | false        | Up to 5 extra text snippets per result (paid)                                             |
| `text_decorations` | bool   | No       | true         | Bold/italic markup in snippets                                                            |
| `spellcheck`       | bool   | No       | true         | Suggest corrected query                                                                   |

### Response (relevant fields only)

```json
{
  "query": {
    "original": "string",
    "altered": "string | null",
    "spellcheck_off": bool
  },
  "web": {
    "results": [
      {
        "title": "string",
        "url": "string",
        "description": "string",
        "age": "string",                    // "2 hours ago"
        "page_age": "string",              // ISO 8601
        "extra_snippets": ["string"],
        "meta_url": {
          "hostname": "string",
          "favicon": "string"
        }
      }
    ]
  },
  "news": {
    "results": [
      {
        "title": "string",
        "url": "string",
        "description": "string",
        "age": "string",
        "meta_url": { "hostname": "string" }
      }
    ]
  },
  "discussions": {
    "results": [
      {
        "title": "string",
        "url": "string",
        "description": "string",
        "data": {
          "forum_name": "string",
          "num_answers": int,
          "question": "string",
          "top_comment": "string"
        }
      }
    ]
  },
  "infobox": {
    "results": [
      {
        "title": "string",
        "description": "string",
        "long_desc": "string",
        "url": "string",
        "attributes": [["label", "value"]]
      }
    ]
  },
  "faq": {
    "results": [
      {
        "question": "string",
        "answer": "string",
        "title": "string",
        "url": "string"
      }
    ]
  }
}
```

All sections (`web`, `news`, `discussions`, `infobox`, `faq`) are optional in the response - only present when results exist.

### Error Response

```json
{
  "status": int,
  "code": int,
  "detail": "string"
}
```

| Status | Meaning                       |
| ------ | ----------------------------- |
| 401    | Invalid API key               |
| 403    | Plan doesn't support endpoint |
| 422    | Parameter validation failed   |
| 429    | Rate limit exceeded           |

### Rate Limits

| Plan        | Queries/Month | $/Month         | Extra Snippets | Summarizer |
| ----------- | ------------- | --------------- | -------------- | ---------- |
| Free        | 2,000         | 0               | No             | No         |
| Basic       | 20,000        | ~5              | Yes            | No         |
| Pro         | Pay-per-use   | ~3–5/1K queries | Yes            | No         |
| Data for AI | Enterprise    | Contact         | Yes            | Yes        |

Free tier: ~1 req/sec. Paid tiers have higher rate limits.

## Tool Definition (LLM-Facing)

This is the schema exposed to the model via the provider's tool-calling mechanism.

### `web_search`

```json
{
  "name": "web_search",
  "description": "Search the web for current information. Use this when you need up-to-date facts, recent events, documentation, or anything beyond your training data.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "The search query. Be specific and use keywords."
      },
      "count": {
        "type": "integer",
        "description": "Number of results to return (1-20).",
        "default": 5
      },
      "freshness": {
        "type": "string",
        "description": "Filter by recency: 'pd' (past day), 'pw' (past week), 'pm' (past month), 'py' (past year)."
      }
    },
    "required": ["query"]
  }
}
```

Keep the tool schema minimal. The agent doesn't need `country`, `search_lang`, `safesearch`, or `result_filter` - those are set by the runtime based on user config.

### Parameter Mapping

When the agent calls `web_search`, the runtime maps it to a Brave API request:

| Tool Parameter  | Brave Parameter    | Notes                                        |
| --------------- | ------------------ | -------------------------------------------- |
| `query`         | `q`                | Direct pass-through                          |
| `count`         | `count`            | Clamp to 1–20                                |
| `freshness`     | `freshness`        | Pass-through if set                          |
| _(from config)_ | `country`          | User config or default `"US"`                |
| _(from config)_ | `search_lang`      | User config or default `"en"`                |
| _(hardcoded)_   | `safesearch`       | Always `"moderate"`                          |
| _(hardcoded)_   | `text_decorations` | Always `false` (cleaner for LLM consumption) |
| _(hardcoded)_   | `result_filter`    | `"web,news,discussions,faq,infobox"`         |

### Response Mapping

The raw Brave response is too verbose for the model's context window. The runtime should flatten and trim it before returning as the tool result.

**Flattened tool result format:**

```json
{
  "results": [
    {
      "type": "web",
      "title": "string",
      "url": "string",
      "snippet": "string",
      "age": "string"
    },
    {
      "type": "news",
      "title": "string",
      "url": "string",
      "snippet": "string",
      "age": "string"
    },
    {
      "type": "discussion",
      "title": "string",
      "url": "string",
      "forum": "string",
      "question": "string",
      "top_answer": "string"
    },
    {
      "type": "faq",
      "question": "string",
      "answer": "string",
      "url": "string"
    }
  ],
  "infobox": {
    "title": "string",
    "description": "string",
    "attributes": [["label", "value"]]
  },
  "query_corrected": "string | null"
}
```

**Mapping rules:**

1. Web results: `description` → `snippet`. Drop `meta_url`, `page_age`, `thumbnail`, `extra_snippets`.
2. News: same as web.
3. Discussions: extract `data.forum_name` → `forum`, `data.top_comment` → `top_answer`.
4. FAQ: pass through as-is.
5. Infobox: take first result only. Drop `profiles`, `ratings`, `images`.
6. Set `query_corrected` from `query.altered` if present.
7. Cap total results at 10 to limit token usage.

## Implementation Notes

1. **API key source.** Read from `BRAVE_API_KEY` env var or user config file. Never hardcode.
2. **Timeout.** Set HTTP timeout to 10 seconds. Brave is fast but network issues happen.
3. **Retry.** Retry once on 429 (rate limit) with a 1-second backoff. Don't retry on 4xx errors.
4. **Token budget.** The flattened result for 10 results is ~1K–2K tokens. This leaves plenty of room in the context window.
5. **No summarizer.** The summarizer endpoint requires an enterprise plan and a two-step flow. Skip it - the LLM can synthesize results itself.
6. **`text_decorations: false`** - Brave adds `<strong>` tags to snippets by default. Disable for cleaner LLM input.
