# Search and Extraction

## Search Modes

`thndrs` supports three web-search modes through `--websearch`:

- `native`
- `exa`
- `none`

## Umans Native Search

`native` uses Umans server-side native search. It is the default mode.

## Exa Search

`exa` asks Umans to use its Exa-backed search path.

## Disabled Search

`none` disables Umans server-side search.

## Local Extraction

Local extraction is used for deterministic page inspection and fallback paths.
It follows the same read-only posture as the repository tools.

## Public URL Safety

Local URL reads only accept public `http` and `https` URLs. Private-network
targets are rejected by default, and redirects, content type, response size, and
timeouts are bounded.

## Truncation Metadata

Search and extraction results carry truncation metadata so the transcript can
show when output was capped.

## Search Transcript Entries

Search activity renders through normal tool transcript entries, whether search
is handled by Umans or a local fallback.
