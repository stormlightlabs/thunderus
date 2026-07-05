---
title: "OpenCode Zen"
Source: https://opencode.ai/docs/zen/
Author: OpenCode / Anomaly
Date: "Last updated 2026-07-05"
Captured: 2026-07-05
Tags: providers, llm-api, opencode, zen, free-models
---

## Summary

OpenCode Zen is an optional OpenCode AI gateway that exposes a curated model
catalog through direct API endpoints, including Big Pickle as a free
OpenAI-compatible chat-completions model.

## Key Ideas

- **Provider shape:** Zen works like an OpenCode provider: users sign in,
  create or copy an API key, and select models from the Zen catalog.
- **Curated routing:** OpenCode says it tests model/provider combinations,
  works with providers, benchmarks the results, and recommends the combinations
  it is willing to serve.
- **Model namespace:** OpenCode config model ids use the `opencode/<model-id>`
  form. For Big Pickle, the config-facing id is `opencode/big-pickle`.
- **Direct API endpoints:** The catalog uses multiple endpoint families:
  Responses, Anthropic Messages, Google model routes, and OpenAI-compatible
  chat completions.
- **Big Pickle route:** Big Pickle has raw id `big-pickle` and uses
  `https://opencode.ai/zen/v1/chat/completions` with the OpenAI-compatible AI
  SDK package.
- **Free with caveats:** Big Pickle is listed as free for input, output, and
  cached reads. OpenCode also describes the free period as limited.
- **Privacy caveat:** OpenCode says Big Pickle data may be used to improve the
  model during the free period.

## Claims & Evidence

### Zen is a curated optional provider.

The Zen page describes it as a tested and verified model list provided by the
OpenCode team, and says users do not need Zen in order to use OpenCode.

Confidence: high.

### Big Pickle is present in the live model catalog.

`GET https://opencode.ai/zen/v1/models` returned a model list containing
`big-pickle` with `owned_by: "opencode"` on 2026-07-05.

Confidence: high for the capture date; the catalog is dynamic.

### Big Pickle currently uses an OpenAI-compatible chat-completions route.

The endpoint table maps Big Pickle to
`https://opencode.ai/zen/v1/chat/completions` and the OpenAI-compatible SDK
package.

Confidence: high for the current docs.

### Big Pickle is a prominent free model in the catalog.

The pricing table marks Big Pickle as free. The pricing notes describe it as a
stealth model that is free for a limited time.

Confidence: high for current pricing.

### Big Pickle has a different privacy caveat during the free period.

OpenCode says Big Pickle data may be used to improve the model during the free
period.

Confidence: high for the stated policy.

## Important Terms

| Term                       | Meaning                                                                     |
| -------------------------- | --------------------------------------------------------------------------- |
| OpenCode Zen               | OpenCode's optional AI gateway for curated model/provider combinations.     |
| `opencode/<model-id>`      | OpenCode config model id namespace for Zen models.                          |
| `opencode/big-pickle`      | Config-facing Big Pickle model id in OpenCode.                              |
| `big-pickle`               | Raw model id used in direct Zen API requests.                               |
| `/zen/v1/chat/completions` | Zen endpoint family for Big Pickle and other OpenAI-compatible chat models. |
| `/zen/v1/models`           | Model discovery endpoint for the current Zen catalog.                       |
| Limited free period        | OpenCode's wording for the free availability window.                        |

## Questions for Review

- What is the config-facing model id for Big Pickle?
- Which Zen endpoint family does Big Pickle use?
- How does OpenCode describe Big Pickle's free availability?
- What privacy caveat applies to Big Pickle during its free period?
- Which endpoint can verify whether Big Pickle remains in the catalog?

## Connections

- Related ideas: OpenAI-compatible provider adapters, dynamic model discovery,
  provider privacy labeling, pricing-aware model selection.
- Related sources: OpenCode Go provider notes, provider protocol mapping.
- Contradictions or tensions: Big Pickle is listed as free, while the docs also
  use "limited time" wording and apply a free-period data-use caveat.
- Useful applications: Zen's model namespace, endpoint mapping, model
  discovery, and privacy/pricing caveats are enough to evaluate provider
  integration designs.

## Open Questions

- Does the same API key work for both OpenCode Go and general OpenCode Zen
  endpoints?
- What exact auth header format does Zen expect for direct API calls?
- Does Big Pickle support streaming tool calls in the same wire shape as
  upstream OpenAI-compatible chat completions?
- What error shape indicates that Big Pickle's free period ended or that the
  model is unavailable to the workspace?
- How long has the limited-free notice been present, and does OpenCode publish
  any additional commitment about Big Pickle's availability?

## Takeaways

- Big Pickle is the clearly documented free OpenCode Zen model in this capture.
- Big Pickle maps to raw model `big-pickle` on `/zen/v1/chat/completions`.
- Any use of Big Pickle should preserve the free-period pricing and privacy
  caveats from the source docs.
