---
title: "ChatGPT Codex Provider"
---

ChatGPT Codex models use the `chatgpt-codex/<model-id>` form:

```toml
model = "chatgpt-codex/gpt-5.5"
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

Setup and login share the same authentication path. They use device-code
authentication first and fall back to a browser PKCE flow with a localhost
callback when needed. Stored credentials are refreshed before provider requests
when possible.

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

- `chatgpt-codex/gpt-5.5`
- `chatgpt-codex/gpt-5.4`
- `chatgpt-codex/gpt-5.4-mini`
- `chatgpt-codex/gpt-5.3-codex-spark`

The provider labels these entries as ChatGPT-backed and experimental in status
copy to help distinguish them from OpenAI Platform API-key (coming soon) routes.
