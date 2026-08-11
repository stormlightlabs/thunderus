# Session Context Tasks

## SESSION-1: Fork a session from a settled turn

- [ ] Select only replayable settled turn boundaries.
- [ ] Assign a new ID and record source session, source turn, time, and lineage.
- [ ] Store a self-contained replayable prefix without pending tools,
      permissions, queues, or processes.

## SESSION-2: Export sessions for human review

- [ ] Export deterministic Markdown and self-contained HTML.
- [ ] Preserve semantic messages, reasoning summaries, tools, status, errors,
      findings, session identity, and lineage with bounded redacted details.
- [ ] Require no external scripts or assets in HTML.

## CONTEXT-1: Inspect context at a provider request

- [ ] Select a turn, request, and retry without exposing raw provider payloads.
- [ ] Show item kind, source, estimated size, visibility, lifecycle, omission
      reason, budgets, route, model, usage, and compaction boundary.
- [ ] Distinguish historical capture from the next-request projection.
- [ ] Keep content-free history bounded and useful after resume.

## CONTEXT-2: Compare context between requests

**Blocked by:** CONTEXT-1.

- [ ] Group additions, removals, lifecycle changes, replacements, truncation,
      and compaction.
- [ ] Compare budgets, serialized size, estimated tokens, and reported usage.
- [ ] Use one comparison model across turns and forked sessions.

## CONTEXT-3: Browse session lineage

**Blocked by:** SESSION-1.

- [ ] Show source turn, title, model, activity, and lock or corruption state.
- [ ] Inspect, resume, fork, or export through established workflows.
- [ ] Diagnose missing parents, malformed lineage, and cycles.

## CONTEXT-4: Capture opt-in request projections

**Blocked by:** CONTEXT-1 and an approved privacy and retention design.

- [ ] Default off and require a per-run choice.
- [ ] Store normalized projections without credentials or raw payloads.
- [ ] Fail closed on size and retention limits and preserve redaction in inspect
      and export surfaces.
