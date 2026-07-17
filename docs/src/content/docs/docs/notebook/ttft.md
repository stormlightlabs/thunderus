---
title: "Time to First Token"
sources:
  - https://arxiv.org/abs/2410.08391
  - https://arxiv.org/abs/2606.17104
Author: Maxwell Horton et al.; Shun Usami, Venkatram Vishwanath, E. Wes Bethel
Date: 2024-10-10
Captured: 2026-07-05
Tags: observability, latency, streaming, llm
---

## Summary

Time to first token is the user-visible latency between submitting an LLM turn
and receiving the first generated output token or stream event.

## Key Ideas

- **TTFT measures first response latency:** In transformer inference, the first
  output appears after prompt processing/prefill and initial generation.
- **TTFT is distinct from throughput:** A model can start quickly and then
  decode slowly, or start slowly and then stream quickly. TTFT should not be
  collapsed into total duration or tokens per second.
- **Prefill dominates first-token delay:** Research commonly maps TTFT to the
  prefill phase, while decode performance is captured by time per output token
  or similar metrics.
- **Client measurement includes client overhead:** Application-observed TTFT
  should be explicit about measuring from local submit time to first stream
  event, not pure provider-side inference time.
- **Streaming UX value:** Surfacing TTFT gives users a compact answer to "did
  the model start responding yet?" without waiting for the full response.

## Claims & Evidence

### TTFT is the delay before the first generated output.

Horton et al. describe transformer inference as beginning with prompt
processing and identify time spent producing the first output as time to first
token.

Confidence: high.

### TTFT and TPOT map to different phases of inference.

Usami, Vishwanath, and Bethel describe modern inference systems as decomposed
into Prefill and Decode phases, with TTFT and TPOT commonly used as the
corresponding latency metrics.

Confidence: high.

### Client-observed TTFT is useful but should be labeled clearly.

Application-observed TTFT includes local request construction, network latency,
provider scheduling, prefill, and the first received streaming event. That
value is useful for user experience, but it is not the same as pure
provider-side inference time.

Confidence: medium. This is implementation guidance inferred from the metric
definition.

## Important Terms

| Term                 | Meaning                                                              |
| -------------------- | -------------------------------------------------------------------- |
| TTFT                 | Time to first token; elapsed time until the first generated output.  |
| TPOT                 | Time per output token; decode-phase pacing after generation begins.  |
| Prefill              | Prompt-processing phase before normal token-by-token decode.         |
| Decode               | Autoregressive generation phase after the first output is available. |
| Client-observed TTFT | Local elapsed time from submit to first received stream event.       |

## Questions for Review

- What event should start a client-observed TTFT timer?
- Which received stream events should stop the timer?
- Why is TTFT different from total turn duration?
- Why should an application label this as client-observed TTFT rather than
  provider-only inference TTFT?

## Connections

- Related ideas: streaming events, provider normalization, latency diagnostics,
  model selection.
- Related sources: Provider protocol mapping notes, OpenCode Zen notes.
- Contradictions or tensions: TTFT is often an inference-engine metric, but
  applications can only measure an end-to-end client-observed value unless
  providers expose server timing.
- Useful applications: show a pending first-token state while a request is
  waiting, then render a compact value such as `842ms` once the first semantic
  model output arrives.

## Open Questions

- Should tool-call deltas count as the first token event if no assistant text
  or reasoning text appears first?
- Should a provider status event stop the TTFT timer, or only semantic model
  output events?
- Should application logs persist TTFT per turn, or should it remain live UI
  state until a separate observability feature needs it?
- How should retries be represented: one TTFT for the successful attempt or a
  total user-observed TTFT across retries?

## Takeaways

- Client-observed TTFT is measured from local request submission to the first
  semantic model-output event.
- Display TTFT as a user-observed latency value, not as a provider-internal
  benchmark.
- Keep TTFT separate from token counts, total duration, and per-token decode
  speed.
