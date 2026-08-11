---
title: Token Optimization
description: How thndrs optimizes tokens
---

`thndrs` treats token optimization as a set of explicit context decisions. It
does not offer a quality or economy preset, and it does not silently discard
model-visible work.

## What changes a request

The context ledger records each item considered for a request, including its
visibility, reason, estimated size, protection, and recovery handle when one
exists. Ordinary selection keeps the active working set within the configured
context budget. Deterministic reducers can shorten eligible tool output while
preserving their required diagnostics and recovery metadata.

Token counts produced locally are estimates. Provider-reported input, output,
and cache components remain separate measurements when an adapter supplies
them.

## Range compression

`/compact` asks the configured model to summarize a closed transcript range.
The model must return a versioned JSON summary with its objective, findings,
decisions, paths, failures, verification, blockers, source metadata, and every
protected fact from the range. `thndrs` rejects malformed summaries and any
summary that changes a source hash, recovery handle, source sequence, protected
fact, or prior-summary reference.

An approved summary replaces only the transcript entries it covers in the next
model projection. The canonical transcript stays append-only. Recent complete
turns remain verbatim when the conversation is large, and a later compaction
merges the previous anchored summary with only the newly closed history.

Ranges containing tool output, failures, permission state, or unresolved work
wait for `/context review approve` or `/context review reject`. Rejection and
model failure leave the active projection unchanged. A successful compaction
record includes the local before/after byte and token estimates; provider usage
and cache values appear only when the provider reports them.

See [Prompting and Input](/docs/usage/prompting-and-input/) for the commands and
[Session Format](/docs/reference/session-format/) for the durable audit record.
See [Configuration](/docs/reference/configuration/#context-compaction) for
automatic triggering and recent-tail controls.
