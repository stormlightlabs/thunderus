---
title: "Web Search"
---

## Search Backends

`thndrs` owns web search and exposes three backend settings through
`--websearch` or the `websearch` config key:

- `duckduckgo` (the default): query DuckDuckGo's HTML endpoint.
- `searxng`: query a configured SearXNG instance using its JSON API.
- `none`: disable the `web_search` tool.

The selected backend is part of runtime and session metadata. Search is never
inferred from prompt wording and no provider-specific search header is sent.

## SearXNG

SearXNG requires an HTTP(S) base URL. Configure it with `--websearch-url`,
`websearch_url`, or `THNDRS_WEBSEARCH_URL`:

```sh
thndrs --websearch searxng --websearch-url http://127.0.0.1:8080
```

The application requests `/search?q=...&format=json` and normalizes the
returned title, URL, and snippet fields. The configured SearXNG host may be
loopback or private because it is an explicit local service. SearXNG result
URLs remain untrusted and are fetched only through the public URL policy.

## Result handling

Results are capped at ten per call (five by default), then each selected public
URL is fetched through the bounded Lectito-backed extraction path. Results keep
their original order; an extraction failure is reported next to that result so
later results can still be used. Search and extraction output includes
truncation and transport diagnostics when relevant.

Search results should usually be paired with `read_url` before making claims
about page content.

## Local Extraction

Extraction follows the same read-only posture as the repository tools.

HTML and XHTML pages are extracted with Lectito into readable Markdown and plain
text. Other allowed text-like content types, such as JSON, XML, plain text, feeds,
YAML, CSV, and JavaScript, are returned as raw text. Binary content is rejected.

## Public URL Safety

Local URL reads only accept public `http` and `https` URLs. Private-network targets
are rejected before fetching and again after redirects. Redirect count, content type,
response size, and total request time are bounded.

This policy also applies to result URLs returned by SearXNG.

## Truncation Metadata

Search and extraction results carry truncation metadata so the transcript can
show when output was capped.

## Search Transcript Entries

Search activity renders through normal tool transcript entries and remains provider-neutral.
