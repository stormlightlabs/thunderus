---
title: "Search and Extraction"
---

## Search Modes

`thndrs` supports four web-search modes through `--websearch`:

- `auto`
- `native`
- `exa`
- `none`

The selected mode is part of the runtime snapshot so the assistant can explain
which search path is active for the current turn.

## Auto Search

`auto` resolves per prompt. If the prompt appears to need current external
information, `thndrs` resolves it to provider-native search. Otherwise it
resolves to `none` and leaves only local tools available.

## Umans Native Search

`native` uses Umans server-side native search. The provider request carries the
`native` search header.

## Exa Search

`exa` asks Umans to use its Exa-backed search path. The provider request carries
the `exa` search header.

## Disabled Search

`none` disables Umans server-side search. The model can still call the local
`web_search` tool when the tool catalog is available.

## Local Search

Local `web_search` uses DuckDuckGo's HTML endpoint as a fallback path. Results
are intentionally small and meant to identify pages worth reading, not to
replace source inspection.

Search results should usually be paired with `read_url` before making claims
about page content.

## Local Extraction

Local extraction is used for deterministic page inspection and fallback paths.
It follows the same read-only posture as the repository tools.

HTML and XHTML pages are extracted with Lectito into readable Markdown and plain
text. Other allowed text-like content types, such as JSON, XML, plain text,
feeds, YAML, CSV, and JavaScript, are returned as raw text. Binary content is
rejected.

## Public URL Safety

Local URL reads only accept public `http` and `https` URLs. Private-network
targets are rejected before fetching and again after redirects. Redirect count,
content type, response size, and total request time are bounded.

## Truncation Metadata

Search and extraction results carry truncation metadata so the transcript can
show when output was capped.

## Search Transcript Entries

Search activity renders through normal tool transcript entries, whether search
is handled by Umans or a local fallback.
