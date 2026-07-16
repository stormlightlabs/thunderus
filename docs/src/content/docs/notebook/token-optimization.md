---
title: "Token Optimization for Tool-Using Agents"
Author: TokenWarden, OpenCode DCP, Anthropic, OpenTelemetry
Date: 2026-07-16
Captured: 2026-07-16
Tags: [token-optimization, context-management, coding-agent, tool-output, prompt-caching]
Sources:
  - https://tokenwarden.ai/
  - https://github.com/Opencode-DCP/opencode-dynamic-context-pruning
  - https://platform.claude.com/docs/en/build-with-claude/context-editing
  - https://platform.claude.com/docs/en/agents-and-tools/tool-use/manage-tool-context
  - https://platform.claude.com/docs/en/build-with-claude/cache-diagnostics
  - https://github.com/open-telemetry/semantic-conventions/blob/main/docs/gen-ai/gen-ai-metrics.md
---

## Summary

Token optimization in tool-using agents covers several distinct problems:
reducing verbose inputs, limiting fixed prompt overhead, removing context that
has outlived its usefulness, choosing material under a context limit, and
accounting for prompt-cache effects. These techniques act at different stages
and have different correctness, cost, latency, and audit tradeoffs.

No reduction percentage is meaningful on its own. A useful evaluation compares
token usage with task results, tool calls, latency, cache behavior, and the
ability to recover omitted evidence.

## Key Findings

- **Tool definitions and results are separate sources of pressure.** Anthropic's
  tool-context guidance distinguishes tool discovery, programmatic calling,
  prompt caching, and old-result clearing because each targets a different part
  of the request.
- **Initial reduction and later pruning are different operations.** A tool
  result can be concise when first added and still become irrelevant later in a
  long interaction.
- **Prompt caching does not reduce context size.** It may lower repeated input
  cost and latency, but cached tokens still occupy context. Editing an earlier
  prefix can invalidate reuse.
- **Exact mechanical transformations are easier to validate than semantic
  summaries.** Repeated-line collapse or terminal-control stripping can have
  explicit preservation rules; summary quality depends on model behavior and
  source material.
- **Durable history and request content need not be identical.** OpenCode DCP
  and Anthropic context editing both describe request-time transformations that
  leave the client's stored conversation unchanged.
- **Vendor benchmarks are workload-specific.** TokenWarden reports up to 70%
  fewer input tokens in its benchmark, which motivates measurement but does not
  establish a baseline for another agent or workload.

## Claims And Evidence

### Tool Context Has Multiple Cost Centers

Anthropic identifies four mechanisms for tool-heavy agents:

| Mechanism            | Primary target                            | Main tradeoff                                                     |
| -------------------- | ----------------------------------------- | ----------------------------------------------------------------- |
| Tool search          | Definitions loaded before they are needed | Additional discovery step                                         |
| Programmatic calling | Intermediate tool-result round trips      | More orchestration moves into executable code                     |
| Prompt caching       | Repeated stable definitions and prefixes  | Context is not smaller, and cache semantics are provider-specific |
| Context editing      | Old results in conversation history       | Removed material may be needed later                              |

Caveat/confidence: High for the decomposition. The cited mechanisms are
provider features, not a universal agent API.

### Request-Time Pruning Can Preserve Stored History

OpenCode DCP says it replaces pruned request content with placeholders while
leaving session history unchanged. Anthropic says its server-side context
editing is applied before inference while the client retains the full
conversation.

Caveat/confidence: High. Retaining history creates separate privacy, storage,
retention, and access-control questions.

### Context Editing And Prompt Caching Interact

Anthropic documents that clearing earlier tool results invalidates the cached
prefix from the edit point. Its `clear_at_least` setting avoids breaking the
cache for a trivial edit, and cache diagnostics identify the earliest request
divergence.

Caveat/confidence: High for prefix-sensitive caches. The cost of a miss varies
by provider, model, cache lifetime, and pricing.

### Token Metrics Need Measurement Provenance

OpenTelemetry recommends its token-usage metric only when counts are readily
available or can be obtained efficiently. If instrumentation cannot obtain the
counts, it should not report the metric as though it were observed.

Caveat/confidence: High. OpenTelemetry's GenAI conventions are still evolving,
and an application may use a different internal or exported schema.

## Technique Catalogue

### Reduce Tool Output Before First Use

Possible transformations include:

- stripping terminal escape sequences and progress redraws;
- collapsing exact repeated lines with a repetition count;
- limiting blank-line runs;
- extracting machine-readable compiler or test diagnostics;
- retaining failures and summaries while omitting routine successful lines;
- limiting result size and indicating truncation;
- retaining a reference to omitted material where recovery is supported.

The preservation contract depends on the tool family. A compiler result may
need file locations, error codes, and source excerpts; a search result may need
rank, source identity, and snippets; a test result may need failed test names
and captured output.

### Avoid Resending Identical Material

Identity rules may use:

- file path, range, and content hash for reads;
- query, filters, source revision, and freshness for searches;
- arguments plus relevant repository or environment state for commands;
- a tool-specific cache key for remote calls.

Using only tool name and arguments can be unsafe. The same command before and
after a source change can produce evidence about different states.

### Retire Old Context

Possible policies include:

- chronological removal of old tool results;
- removal of exact duplicates;
- replacement of superseded source reads;
- removal of failed-tool inputs while keeping the failure;
- replacement of a contiguous range with a summary;
- explicit model- or user-selected pruning;
- budget-based selection without rewriting the stored conversation.

Each approach answers a different question. Chronological removal is simple but
can lose old decisions. Semantic selection may retain important material but is
harder to predict and audit. User control improves transparency but adds
interaction cost.

### Reduce Fixed Tool Overhead

Large tool catalogues can be handled by:

- sending every definition
- selecting definitions using application rules
- exposing tool groups first and loading schemas on demand
- using a provider's native tool-search feature
- caching a stable catalogue prefix

Lazy discovery reduces baseline context at the cost of an additional discovery
step and different behavior across providers.

### Summarize Conversation Ranges

Summary designs vary along several axes:

- whole conversation versus selected range;
- free-form prose versus a typed schema;
- automatic versus model-initiated versus user-initiated;
- immediate application versus review;
- flat summaries versus summaries that retain source relationships;
- recoverable versus destructive replacement.

OpenCode DCP supports contiguous-range compression and experimental
message-level compression. It nests an earlier overlapping summary inside a
later range so earlier information is not silently discarded.

## Measurement Vocabulary

| Label                  | Meaning                                                        |
| ---------------------- | -------------------------------------------------------------- |
| Provider-reported      | Returned by the inference provider                             |
| Locally counted        | Calculated by a named tokenizer or counting method             |
| Estimated              | Calculated by a documented heuristic                           |
| Before/after reduction | Difference produced by the same method on both representations |
| Cached input           | Provider-specific count of input served from cache             |
| Cache creation         | Provider-specific count of input added to cache                |
| Cost estimate          | Token measurements combined with a dated pricing source        |

A heuristic estimate before reduction should not be subtracted from a
provider-reported value after reduction and described as exact savings.

## Evaluation Pattern

Useful measurements include:

- input, cached-input, and output tokens
- exact serialized bytes
- task or verification result
- tool calls, retries, and recovery calls
- missed diagnostics and incorrect actions
- total latency and time to first output
- prompt-cache reuse after a transformation

## Design Questions

- Is omitted evidence retained, and under what privacy and retention policy?
- Who decides that context is no longer useful: deterministic policy, model,
  user, or provider?
- What information must each tool-family reducer preserve?
- How is a false duplicate prevented when repository or remote state changes?
- When is a cache-invalidating edit large enough to be worthwhile?
- Are summaries reviewable, attributable to sources, and recoverable?
- Which counts are estimates, locally counted values, or provider reports?
- What correctness signal prevents a smaller prompt from being mistaken for a
  better agent?

## Failure Modes

- Optimizing token count instead of task completion.
- Treating prompt caching as context reduction.
- Deduplicating stateful operations by arguments alone.
- Removing old material without a recovery or provenance policy.
- Applying one generic reducer to tool families with different evidence needs.
- Repeatedly summarizing summaries until source details are untraceable.
- Reporting vendor benchmark savings as a general forecast.
- Reporting estimates as provider usage or exact monetary savings.
- Ignoring the cache, latency, and extra-tool-call cost of pruning.

## Takeaways

- Token optimization is a set of independent mechanisms, not one mode.
- Tool-output reduction, lifecycle pruning, working-set selection, and caching
  should be evaluated separately.
- Exact reductions are easier to validate; semantic reductions need stronger
  provenance and quality checks.
- The best policy depends on provider capabilities, tool mix, workload, and
  acceptable user-control tradeoffs.
- Token savings are credible only alongside correctness, latency, cache, and
  recovery measurements.
