---
title: "Context Observability for Tool-Using Agents"
Author: OpenTelemetry, Anthropic, OpenCode DCP, TokenWarden
Date: 2026-07-16
Captured: 2026-07-16
Tags: [context-observability, token-usage, telemetry, coding-agent, privacy]
Sources:
  - https://github.com/open-telemetry/semantic-conventions/blob/main/docs/gen-ai/gen-ai-metrics.md
  - https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/
  - https://opentelemetry.io/blog/2026/genai-observability/
  - https://platform.claude.com/docs/en/api/go/messages
  - https://platform.claude.com/docs/en/build-with-claude/context-editing
  - https://platform.claude.com/docs/en/build-with-claude/cache-diagnostics
  - https://github.com/Opencode-DCP/opencode-dynamic-context-pruning
  - https://tokenwarden.ai/
---

## Summary

Context observability explains how a model request was assembled and how that
request behaved. It can cover candidate context, selected context,
transformations, provider usage, caching, latency, omission reasons, and
recovery. These concerns extend beyond ordinary model-call tracing because much
of the important behavior occurs before the provider request is sent.

An observability design can be entirely local, externally exported, or both.
OpenTelemetry supplies useful GenAI vocabulary for provider operations and
metrics, but it does not define every context-selection or pruning concept an
agent may need.

## Key Findings

- **Provider usage alone does not explain request assembly.** Aggregate input
  and output tokens cannot identify which source occupied the prompt or why an
  item was omitted.
- **Candidate, projected, and provider measurements are different views.** A
  candidate inventory describes possible context; a projection describes what
  the client prepared; provider usage describes the actual inference call under
  provider-specific semantics.
- **Missing measurements should remain missing.** OpenTelemetry says token
  metrics should not be emitted when counts cannot be obtained efficiently.
- **Provider usage fields require normalization.** Providers may report input,
  cached input, cache creation, reasoning, or modality components differently.
- **Content capture is sensitive.** Prompts, tool arguments, results, system
  instructions, paths, and source excerpts may contain secrets, personal data,
  or proprietary code.
- **Context changes are useful audit events.** Pruning, summarization,
  deduplication, recovery, and user overrides can be represented as explicit
  occurrences rather than hidden prompt mutation.

## Claims And Evidence

### OpenTelemetry Covers Provider Operations And Token Metrics

The GenAI semantic conventions define or discuss model-operation attributes,
token-usage metrics, operation duration, time to first chunk, and time per
output chunk. These provide a common vocabulary for observing inference calls.

Caveat/confidence: High. The conventions are under development, and using them
is an interoperability choice rather than a requirement for an agent's internal
schema.

### OpenTelemetry Does Not Fully Describe Context Policy

Provider-call telemetry can say which model ran, how long it took, and how many
tokens were reported. It does not by itself explain a harness's candidate
inventory, selection rules, pruning decisions, summary provenance, or recovery
mechanism.

Caveat/confidence: High. These application-level concepts vary across agents.

### Provider Token Totals Can Have Different Shapes

The Anthropic Messages reference describes total request input as the sum of
`input_tokens`, `cache_creation_input_tokens`, and `cache_read_input_tokens`.
OpenTelemetry's normalized GenAI input count includes cached input. Other
providers may expose a top-level inclusive total or different decompositions.

Caveat/confidence: High for the cited interfaces. Every provider integration
needs its own documented interpretation.

### Content Capture Should Not Be Assumed Safe

OpenTelemetry warns that system instructions, messages, tool arguments, tool
results, and definitions can be sensitive or large. Its GenAI observability
guidance describes metadata-only capture by default, with prompt and tool
content enabled separately.

Caveat/confidence: High. A local-only tool still needs explicit retention,
redaction, access, and deletion behavior.

### Context Transformations Can Report Applied Effects

Anthropic context editing reports which edits were applied and how many tool
uses or input tokens were cleared. OpenCode DCP exposes context breakdowns,
pruning statistics, decompression, and recompression controls.

Caveat/confidence: High for the cited products. Their event names and state
models should not be treated as a universal schema.

## Observation Layers

### Candidate Inventory

Possible fields include:

- stable local identity
- kind or source category
- source size and estimated size
- content hash or revision where safe
- inclusion decision and reason
- replacement or summary relationship
- recoverability and retention state

An inventory is useful when users need to answer why a source was included or
excluded. It may be unnecessary for a one-shot application with no dynamic
context policy.

### Request Projection

A projection record may describe:

- system/instruction bytes
- tool-definition bytes
- history and current-input bytes
- transformation receipts
- serialized request bytes
- a local estimate or locally counted value
- policy and serializer versions

Recording the full projection provides the clearest audit but carries the
largest privacy and storage risk. Metadata-only inventories reduce that risk
but cannot reproduce exactly what the model saw.

### Provider Operation

Common observations include:

- provider and requested/response model;
- operation start, first output, and completion;
- finish reason and retry count;
- input, output, reasoning, cache-read, and cache-creation counts when reported;
- provider request/response identifiers where safe;
- error type.

This layer may be represented using OpenTelemetry spans and metrics, a local
event log, a provider-specific trace, or another schema.

### Tool Execution

Useful metadata can include:

- tool name or bounded category;
- duration and outcome;
- input and output byte sizes;
- whether output was truncated, transformed, or archived;
- relation to a model request.

Arguments and result bodies are often the most sensitive fields and need not be
captured for duration or size analysis.

### Context Transformation

A transformation observation may record:

- method and version;
- source identities;
- before/after measurements using the same method;
- whether the operation is exact or lossy;
- replacement or summary identity;
- recovery availability;
- duration, outcome, and error type;
- initiator such as policy, model, user, or provider.

Not every application needs explicit transformation records. They become more
valuable as pruning becomes automatic or lossy.

## Measurement Provenance

Token and size observations can come from different sources:

| Source                          | Strength                                    | Limitation                                                           |
| ------------------------------- | ------------------------------------------- | -------------------------------------------------------------------- |
| Exact bytes                     | Reproducible for one serialization boundary | Not directly comparable to billed tokens                             |
| Heuristic estimate              | Cheap and provider-independent              | Approximate and language/content sensitive                           |
| Local tokenizer/counting method | More precise for supported encodings        | Adds model/version coupling and may disagree with provider wrappers  |
| Provider report                 | Best evidence for the actual call           | May arrive late, omit components, or use provider-specific semantics |
| Derived total                   | Enables a common view                       | Correct only when the normalization equation is documented           |

Useful metadata includes the method, version, unit, inclusiveness, and whether
the value is measured or derived. Unknown values should remain absent. Zero
means the source measured zero.

## Provider Normalization

A neutral usage view can preserve both:

- raw provider fields; and
- documented derived values such as inclusive input.

Keeping raw fields makes it possible to revise a normalization rule later.
Before summing cache components, an integration must determine whether the
provider's top-level input already includes them. Streaming and retry behavior
also needs an explicit rule so increments are not counted twice.

## User-Facing Views

- a compact current-context or token indicator
- a request detail page
- a context-item inventory
- a transformation report
- a session history graph
- a machine-readable export
- a human-readable export
- comparison of two policies or requests

The right surface depends on audience and workflow. A persistent dashboard may
help operators but distract interactive users. A command or detail view is
quieter but less discoverable. A model-visible context summary may help the
model recover omitted evidence but consumes context itself.

## Export Options

- application-specific JSON
- human-readable Markdown or text
- OpenTelemetry traces/metrics/logs
- a standardized event envelope with application-specific context payloads
- a bundle containing metadata and selected content

JSON is convenient for tooling; Markdown is convenient for direct review;
OpenTelemetry integrates with existing observability systems. Supporting one
does not require using it as the internal source of truth.

Export policy should state whether it includes:

- only metadata
- the final request projection
- omitted or archived content
- tool arguments/results
- paths and environment information
- raw provider payloads
- secrets after redaction
- schema and policy versions

## Metrics And Cardinality

- provider input/output token distributions
- local projection-size distributions
- reduction ratios by bounded method/category
- difference between local estimates and provider reports
- cache-read ratios and cache-invalidating transformations
- compaction, pruning, and recovery counts
- provider and tool duration
- task or verification outcomes by policy version

Metric dimensions should remain low-cardinality. Session ids, request ids,
paths, context ids, hashes, prompts, and error messages are usually better kept
in local records or sampled traces than metric labels.

## Privacy And Retention Questions

- Is content capture disabled by default?
- Which fields can contain secrets, personal data, or proprietary source?
- Is redaction applied before storage, before export, or both?
- Are paths stored absolutely, relatively, hashed, or omitted?
- How are artifacts capped, expired, deleted, and recovered?
- Can users inspect what was exported and where it was sent?
- Are hashes acting as stable correlators for sensitive content?
- Does enabling content capture require a visible warning or per-session action?
- Can local records and external telemetry have different retention policies?

## Failure Modes

- One cumulative session counter with no per-request attribution.
- Calling every local estimate "usage."
- Assuming all providers define input and cache fields identically.
- Recording only selected items and making omissions invisible.
- Capturing prompt and tool bodies because a telemetry API permits it.
- Treating hashes as anonymous data.
- Using dynamic identifiers or paths as metric labels.
- Measuring token reduction without latency, cache, recovery, or quality data.
- Coupling the internal model to an evolving external convention.
- Building a dashboard before defining accounting and privacy invariants.

## Takeaways

- Context observability spans request assembly, transformations, provider calls, and tools.
- Measurement provenance is as important as the number itself.
- OpenTelemetry is one interoperability option, not a complete context-policy
  model or mandatory internal schema.
- Content capture, retention, and export require explicit privacy decisions.
- The appropriate views and events depend on the agent's context policy,
  provider capabilities, and user workflow.
