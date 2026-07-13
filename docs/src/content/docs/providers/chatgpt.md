---
title: "ChatGPT Codex Provider"
---

ChatGPT Codex models use the `chatgpt-codex/<model-id>` form:

```toml
model = "chatgpt-codex/gpt-5.6-sol"
```

This provider is explicit opt-in. It is ChatGPT-backed and experimental because
it talks to the ChatGPT Codex backend rather than a stable OpenAI Platform API
endpoint.

## Authentication

Use the dedicated ChatGPT Codex OAuth flow:

```sh
thndrs setup --provider chatgpt-codex
thndrs login chatgpt-codex
thndrs logout chatgpt-codex
```

Setup and login share the same authentication path. Browser PKCE is the default
for browser-capable environments where `thndrs` starts a short-lived callback at
`http://localhost:1455/auth/callback`, opens or displays a copyable
authorization URL, and validates the returned state before exchanging the
authorization code.

The TUI accepts the full redirect URL when the callback cannot reach `thndrs`.

Device code is an explicit headless or remote alternative. Select it with:

```sh
thndrs login chatgpt-codex --oauth-method device-code
```

The two methods do not silently fall through to each other.

Stored credentials are refreshed before provider requests when possible.

ChatGPT Codex credentials do not use `OPENAI_API_KEY`. They are ChatGPT
subscription credentials with a bearer access token and ChatGPT account id.
Normal setup does not ask for a ChatGPT API key or access token.

## Credential Storage

`thndrs` stores refreshable ChatGPT Codex credentials in:

```text
~/.thndrs/auth.json
```

That file is sensitive local credential storage. It can contain access tokens,
refresh tokens, expiry timestamps, and account ids. On Unix, `thndrs` writes the
file with `0600` permissions.

For one-off automation or debugging, set:

```sh
export CHATGPT_CODEX_ACCESS_TOKEN=...
```

The environment token overrides stored credentials for the current process and
is not persisted. It still needs to carry the ChatGPT account claim required by
the provider.

## Model Status

Known model picker entries include:

- `chatgpt-codex/gpt-5.6-sol`
- `chatgpt-codex/gpt-5.6-terra`
- `chatgpt-codex/gpt-5.6-luna`
- `chatgpt-codex/gpt-5.5`
- `chatgpt-codex/gpt-5.4`
- `chatgpt-codex/gpt-5.4-mini`
- `chatgpt-codex/gpt-5.3-codex-spark`

The provider labels these entries as ChatGPT-backed and experimental in status
copy to help distinguish them from OpenAI Platform API-key (coming soon) routes.

## GPT-5.6 reasoning

`chatgpt-codex/gpt-5.6-sol`, `chatgpt-codex/gpt-5.6-terra`, and
`chatgpt-codex/gpt-5.6-luna` accept `reasoning_effort = "none"`, `"low"`,
`"medium"`, `"high"`, `"xhigh"`, or `"max"`. The default is `medium`.
Selecting a GPT-5.6 model from `/model` opens the reasoning-effort picker; use
`/reasoning` to change it later. Set
`reasoning_summary = "auto"` to render provider-returned reasoning summaries;
the default `off` preserves opaque reasoning only for the active tool loop.

These controls are available in TOML, through their `THNDRS_` environment
variables, and as ACP session options. They do not persist private provider
payloads in thndrs session files.
