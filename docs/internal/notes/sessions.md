---
title: Coding Agent Session Structures
Sources:
  - https://github.com/openai/codex/
  - https://github.com/sst/opencode/
  - https://github.com/block/goose/
  - https://github.com/Aider-AI/aider/
Author: OpenAI Codex, OpenCode, Goose, Aider maintainers
Date: 2026-06-29
Captured: 2026-06-29
Tags: [coding-agent, sessions, jsonl, persistence, replay, tui]
---

## Summary

There is no single standard coding-agent session format, but mature agents
converge on durable append-only records, explicit session metadata, typed
message/tool parts, separate listing indexes, and replay by reducing events into
UI/model state.

## Key Ideas

- **JSONL is a good local baseline:** Codex persists rollouts as `.jsonl` so
  sessions can be replayed or inspected with normal tools such as `jq`, while
  keeping each line independently parseable.
- **Session metadata should be first-class:** Codex records `SessionMeta`;
  Goose stores session rows with id, working directory, name, type, timestamps,
  provider/model, usage, archive state, and conversation summary fields; OpenCode
  keeps session info in database rows and durable event aggregates.
- **Durable records are not the same as live stream deltas:** OpenCode explicitly
  treats text/reasoning deltas as live-only and persists the completed text or
  reasoning boundary. Codex has both public protocol events and richer raw trace
  events. For `thndrs`, this argues for replayable final chunks plus optional
  transient UI deltas.
- **Tool calls need stable correlation IDs:** Codex raw traces include tool call
  IDs, model-visible call IDs, requester, kind, status, payload references, and
  result payloads. Goose persists tool request/response IDs and provider metadata.
  OpenCode settlement events carry assistant message IDs because provider-local
  call IDs can repeat.
- **Indexes can stay append-only too:** Codex uses an append-only
  `session_index.jsonl` for thread names, where the latest entry wins. This is
  simpler than rewriting session files for renames.
- **SQLite is common after the first cut:** Goose and OpenCode use SQLite for
  richer queries, session lists, search, concurrency, and projections. That does
  not make SQLite necessary for alpha; JSONL can be the durable source first.
- **Aider is intentionally lighter:** Aider logs human-readable Markdown chat
  history and can restore it into messages, while also offering input history and
  LLM history files. This is inspectable but weaker for exact tool replay.

## Claims & Evidence

| Claim                                                                    | Support                                                                                                                                                             | Caveat / Confidence                                                                      |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| No broad coding-agent session file standard exists.                      | Codex, OpenCode, Goose, and Aider all use different persistence models: JSONL rollouts, event-sourced DB rows, SQLite conversations, and Markdown history.          | High; sampled prominent tools, not exhaustive.                                           |
| Append-only records are the safest alpha structure.                      | Codex rollout files and session name index are append-only; OpenCode event streams replay by durable sequence; Aider appends Markdown history.                      | High; append-only recovery is simpler than in-place mutation.                            |
| Store metadata separately from transcript entries.                       | Codex has `SessionMetaLine`; Goose has a `Session` row; OpenCode has `Session.Info`.                                                                                | High; listing/resume should not require rendering the whole transcript every time.       |
| Persist completed content, not every live delta, for replay.             | OpenCode comments mark `Text.Delta` and `Reasoning.Delta` as live-only, with `Text.Ended` and `Reasoning.Ended` as replayable boundaries.                           | High for alpha; raw delta persistence can be added later if needed.                      |
| Tool records need both request and settlement data.                      | Goose separates `ToolRequest` and `ToolResponse`; OpenCode has started/ended/failed events; Codex raw trace has start/runtime/end payloads.                         | High; otherwise resume cannot distinguish pending, failed, or completed tools.           |
| Context and environment snapshots matter for audit.                      | Codex persists turn context such as cwd, approval policy, sandbox policy, model, and network info; OpenCode has Context Epochs for exact privileged system context. | Medium-high; `thndrs` can start with root AGENTS metadata and cwd/model/search settings. |
| A separate search/list index is useful but not required for first JSONL. | Codex uses `session_index.jsonl`; Goose/OpenCode rely on DB queryable metadata.                                                                                     | Medium; alpha can derive list from session headers until performance hurts.              |

## Important Terms

| Term            | Meaning                                                                                                       |
| --------------- | ------------------------------------------------------------------------------------------------------------- |
| Session         | A persistent conversation/work unit with id, cwd/workspace, metadata, transcript, tool events, and run state. |
| Rollout         | Codex term for a persisted replay log of session items in JSONL.                                              |
| Durable event   | A record committed to storage and replayed after restart.                                                     |
| Live delta      | A transient stream fragment useful for connected UI but not necessarily stored.                               |
| Projection      | Derived UI/model state rebuilt from durable records.                                                          |
| Turn            | One user prompt plus the model/tool loop that follows it.                                                     |
| Tool settlement | Durable record that a tool call completed, failed, was interrupted, or was skipped.                           |
| Context epoch   | OpenCode term for an immutable baseline of model-visible system context plus change tracking.                 |

## Practical Shape for `thndrs`

Use a single append-only JSONL file per session for alpha:

```json
{"schema_version":1,"seq":0,"time":"2026-06-29T12:00:00Z","type":"session_meta","session_id":"...","cwd":"/repo","title":"scratch","model":"umans-coder","provider":"umans","websearch":"native"}
{"schema_version":1,"seq":1,"time":"2026-06-29T12:00:02Z","type":"context","sources":[{"kind":"agents_md","path":"/repo/AGENTS.md","sha256":"...","truncated":false}]}
{"schema_version":1,"seq":2,"time":"2026-06-29T12:00:04Z","type":"user","turn_id":"turn_1","text":"explain this repo"}
{"schema_version":1,"seq":3,"time":"2026-06-29T12:00:05Z","type":"assistant_started","turn_id":"turn_1","message_id":"msg_1","model":"umans-coder"}
{"schema_version":1,"seq":4,"time":"2026-06-29T12:00:06Z","type":"tool_started","turn_id":"turn_1","call_id":"call_1","name":"search_text","input":{"pattern":"main"}}
{"schema_version":1,"seq":5,"time":"2026-06-29T12:00:06Z","type":"tool_finished","turn_id":"turn_1","call_id":"call_1","status":"ok","duration_ms":31,"output":{"matches":3,"truncated":false}}
{"schema_version":1,"seq":6,"time":"2026-06-29T12:00:08Z","type":"assistant_finished","turn_id":"turn_1","message_id":"msg_1","text":"...","usage":{"input":0,"output":0,"reasoning":0}}
```

Recommended alpha records:

- `session_meta`: id, created time, cwd, root, title, provider, model,
  websearch mode, app version.
- `context`: loaded source metadata, especially `AGENTS.md` path, scope, hash,
  truncation state, and load errors.
- `user`: prompt text, turn id, optional attachments later.
- `assistant_started`: run/model metadata and assistant message id.
- `assistant_finished`: final replayable assistant text, finish reason, usage,
  provider request id if available.
- `reasoning_finished`: final replayable reasoning text or summary, with provider
  metadata only if safe and useful.
- `tool_started`: call id, turn id, tool name, normalized input, start time.
- `tool_finished`: call id, status, duration, structured output summary,
  truncation state, error text when failed.
- `cancelled` or `failed`: turn id, reason, visible error message.
- `session_renamed`: optional append-only title updates; latest wins.

Avoid for alpha:

- Persisting every text/reasoning delta unless needed for crash debugging.
- Rewriting historical records for rename, summary, or archive state.
- Provider-native raw payloads that may contain secrets or unstable metadata.
- SQLite until session list/search performance or concurrent writers require it.

## Questions for Review

- Which fields are v1-stable: record `type`, `schema_version`, `seq`, and
  `session_id`, or the entire record body?
- Should `assistant_finished.text` include all assistant prose, while streamed
  deltas remain UI-only?
- Do we need `turn_id` from Phase 9, or can `seq` plus message ids carry enough
  structure for alpha?
- Should `session_renamed` and archive state be separate JSONL records or a
  sidecar index like Codex?

## Connections

- Related ideas: Codex rollout JSONL, OpenCode durable/live event split, Goose
  typed conversation parts, Aider's human-readable history.
- Related sources: [pi](./pi.md), [herdr](./herdr.md), [ui-patterns](./ui-patterns.md),
  [agents-md](./agents-md.md), [fs-traversal](./fs-traversal.md).
- Contradictions or tensions: JSONL is simple and inspectable, while richer
  search/listing wants SQLite. Use JSONL as the source of truth first; add a
  derived index later.
- Useful applications: Phase 9 session persistence, v1 inspect/export, resume
  rendering, tool audit, AGENTS.md context audit.

## Open Questions

- Should `thndrs` keep one JSONL file per session under an app data directory, or
  one workspace-local `.thndrs/sessions/` directory?
- How much provider metadata from Umans is stable and safe enough to persist?
- Should failed JSONL lines be skipped with visible warnings, or should a corrupt
  line stop resume until the user exports/repairs?
- Do we need a sidecar `session_index.jsonl` in alpha for title updates and fast
  sidebar listing?

## Takeaways

- Start with per-session append-only JSONL; it is closest to Codex's local
  rollout model and much simpler than an event-sourced SQLite store.
- Persist final replayable boundaries and structured tool settlements; treat live
  stream deltas as UI-only until there is a clear debugging need.
- Include enough metadata on the first line and context records to audit cwd,
  model, search mode, AGENTS.md inputs, tool calls, errors, and resume behavior.
