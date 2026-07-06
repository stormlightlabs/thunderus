---
title: "ChatGPT Codex Provider Research Notes"
Notes: "ChatGPT Codex Provider Research"
Source: "OpenAI Codex docs and local reference implementations"
Author: "OpenAI"
Date: "2026-07-03"
Captured: "2026-07-03"
Tags: ["providers", "chatgpt", "codex", "oauth", "llm-provider"]
---

## Summary

OpenAI presents ChatGPT subscription access for Codex as an OAuth/access-token
credential path for Codex clients, distinct from OpenAI Platform API-key access.

## Key Ideas

- **ChatGPT sign-in is subscription auth:** Codex supports ChatGPT sign-in for
  subscription access and API-key sign-in for usage-based access.
- **API keys remain the ordinary automation path:** OpenAI recommends API-key
  authentication for programmatic Codex CLI workflows such as CI/CD unless the
  workflow specifically needs ChatGPT workspace identity or entitlements.
- **Codex access tokens are workspace-scoped:** Codex access tokens are
  ChatGPT-scoped credentials for trusted local automation and are documented
  for ChatGPT Business and Enterprise workspaces.
- **Custom Codex providers can reuse OpenAI auth:** Codex custom providers can
  set `requires_openai_auth = true` so Codex attaches OpenAI auth instead of a
  provider-specific environment key.
- **Local references separate auth from transport:** Codex and Pi resolve
  credentials into bearer/header auth before request sending, keeping login,
  refresh, request construction, and stream parsing separate.
- **The ChatGPT Codex backend is not the Platform API:** Pi uses
  `https://chatgpt.com/backend-api/codex/responses`, which behaves like a
  Responses-style stream but is not documented as a stable third-party Platform
  API endpoint.

## Claims & Evidence

### Codex supports ChatGPT sign-in and API-key sign-in

OpenAI's Codex authentication docs describe two sign-in paths: ChatGPT sign-in
for subscription access and API-key sign-in for usage-based access. Codex cloud
requires ChatGPT sign-in, while the CLI and IDE extension support both.

Confidence: high.

### ChatGPT subscription credentials are not OpenAI Platform API keys

OpenAI states that API-key usage is billed through the OpenAI Platform account
at standard API rates, while ChatGPT sign-in follows ChatGPT workspace
permissions, RBAC, retention, residency, and plan entitlements.

Confidence: high.

### API-key auth is still the recommended default for generic automation

OpenAI recommends API-key authentication for programmatic Codex CLI workflows,
including CI/CD. Codex access tokens are positioned for trusted automation that
needs ChatGPT workspace access, ChatGPT-managed entitlements, or enterprise
controls.

Confidence: high.

### Codex access tokens are limited to ChatGPT Business and Enterprise workspaces

OpenAI's access-token docs describe Codex access tokens as ChatGPT access tokens
scoped to Codex permissions and say they are currently supported for ChatGPT
Business and Enterprise workspaces.

Confidence: high.

### Custom model providers can use OpenAI auth

OpenAI's Codex auth docs say a custom model provider can set
`requires_openai_auth = true`. With that setting, Codex can use ChatGPT or
API-key auth, and `env_key` is ignored.

Confidence: high.

### A direct ChatGPT Codex provider carries product-contract risk

Pi's OpenAI Codex provider targets the ChatGPT backend Codex Responses endpoint
with OAuth bearer auth and account headers. OpenAI's public docs do not present
that endpoint as a stable Platform API contract for third-party providers.

Confidence: medium.

### Reference implementations favor auth/request separation

Codex resolves provider auth into bearer/header auth before sending model
requests. Pi models credentials separately from transport and refreshes OAuth
credentials through a serialized credential-store mutation.

Confidence: high.

## Important Terms

| Term                    | Meaning                                                                                                        |
| ----------------------- | -------------------------------------------------------------------------------------------------------------- |
| ChatGPT sign-in         | OAuth-style Codex login path that grants access through a ChatGPT account or workspace.                        |
| API-key sign-in         | OpenAI Platform credential path with usage-based billing and API model availability.                           |
| Codex access token      | ChatGPT workspace-scoped token for trusted, non-interactive Codex local workflows.                             |
| `requires_openai_auth`  | Codex custom-provider setting that makes Codex attach OpenAI auth instead of provider-specific env-key auth.   |
| Agent identity          | Codex reference auth mode that can attach task-scoped authorization and ChatGPT account headers.               |
| Backend Codex Responses | ChatGPT backend route used by Pi for Responses-like Codex streaming.                                           |
| Device-code login       | Headless-friendly login flow where the CLI displays a code and the user completes authentication in a browser. |

## Questions for Review

- What are the two OpenAI-supported Codex sign-in paths?
- Why is ChatGPT subscription access different from OpenAI Platform API-key
  access?
- When should a Codex access token be preferred over a Platform API key?
- What does `requires_openai_auth = true` change for a Codex custom provider?
- Which parts of Pi's implementation are reusable design patterns?
- Why does the ChatGPT backend endpoint create product-contract risk?

## Connections

- Related ideas: OAuth credential storage, token refresh, model-provider auth,
  Responses-style event streams, subscription entitlements, workspace controls.
- Related sources: OpenAI Codex authentication docs, Codex pricing docs, Codex
  model docs, Codex access-token docs, Codex model-provider auth code, Pi
  OpenAI Codex provider, Goose OIDC proxy.
- Contradictions or tensions: ChatGPT subscription access is desirable as a
  provider credential, but OpenAI's stable public automation guidance still
  points ordinary programmatic workflows toward Platform API keys.
- Useful applications: an implementation can treat ChatGPT Codex as an
  explicit ChatGPT-backed provider with isolated auth, request construction,
  stream parsing, retry classification, and redaction requirements.

## Open Questions

- Is `https://chatgpt.com/backend-api/codex/responses` intended to remain a
  stable integration endpoint for third-party clients?
- Do personal ChatGPT Plus and Pro accounts have a supported non-interactive
  access-token path, or only browser/device login?
- Which backend request headers are required versus incidental to first-party
  or partner clients?
- What exact error strings identify terminal subscription limits instead of
  retryable throttling?

## Takeways

- Treat ChatGPT subscription access as OAuth/access-token auth, not as
  Platform API-key auth.
- Keep credential storage, token refresh, header derivation, request building,
  and stream parsing separate.
- Label any direct ChatGPT Codex provider as ChatGPT-backed and experimental
  until the backend endpoint has a documented stability contract.
