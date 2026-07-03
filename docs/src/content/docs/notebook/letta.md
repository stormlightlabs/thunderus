---
title: "Letta Memory and Stateful Agents"
description: "Notes on Letta's memory-first agent model, memory blocks, archival memory, shared state, MemFS, dreaming, and stateful agent APIs."
author: Letta maintainers; Charles Packer, Sarah Wooders, Kevin Lin, Vivian Fang, Shishir G. Patil, Ion Stoica, Joseph E. Gonzalez
captured: 2026-07-03
tags: [letta, memgpt, stateful-agents, memory, context-management, archival-memory]
---

## Summary

Letta treats an agent as a durable stateful object rather than a single chat
thread: memory, message history, tool calls, reasoning, and conversation state
persist outside the context window, while selected memory is promoted back into
context through memory blocks, file-backed memory, retrieval tools, and
background reflection.

## Key Ideas

- Letta is the continuation of MemGPT. The open source repository[^6] describes
  Letta as formerly MemGPT and frames it as a platform for agents with advanced
  memory that can learn and self-improve over time.
- The current Letta Agent product is memory-first. Its docs[^1] emphasize using the
  same agent indefinitely across sessions, with the agent learning preferences,
  remembering past interactions, and editing its own memory as it works.
- Letta separates agents from conversations. An agent has durable memory, model
  configuration, and collective message history; a conversation is one thread or
  session with that agent. One agent can participate in many conversations.
- The SDK/API concept of state is broader than memory. Letta persists memories,
  user messages, reasoning, tool calls, runs, steps, and conversations in a
  database so older state can be retrieved after eviction or compaction.
- Memory blocks are the core always-visible memory primitive. They are structured
  prompt sections with a label, description, value, and character limit, and are
  prepended to the model context in an XML-like format.
- Memory block descriptions are control surfaces. The agent uses a block's
  description to decide how to read and write the block, so weak descriptions can
  make memory behavior ambiguous.
- Shared memory blocks turn memory into coordination state. Multiple agents can
  attach to the same block; when one updates it, other attached agents see the
  change.
- Archival memory is the long-term searchable store. It is a semantic database
  for facts, knowledge, and information that should not always occupy context;
  agents use insert/search tools, while developers can manage passages through
  SDK endpoints.
- Letta's context hierarchy is importance- and scale-based. Small, critical
  facts belong in memory blocks; large files can be searched and opened
  partially; less critical long-term memories fit archival memory; very large
  corpora belong in external RAG or MCP-backed tools.
- Current Letta Agent adds MemFS, a git-backed memory filesystem. Agent memory is
  projected into markdown files with YAML frontmatter, version history, conflict
  resolution, and local inspection/editing.
- The `system/` directory in MemFS is the always-loaded tier. Files outside it
  remain visible by path and description, but their full content is loaded only
  when relevant.
- "Dreaming" is Letta's sleep-time compute loop. Background subagents review
  recent conversations and write useful lessons into memory, either after a step
  count or after compaction.
- `/init`, `/remember`, `/doctor`, and `/sleeptime` expose memory management as
  user-facing workflows. Users can bootstrap memory, explicitly teach durable
  facts, audit memory placement/token usage, and configure background reflection.
- The MemGPT paper[^7] supplies the architecture lineage: virtual context management
  moves information between fast and slow memory tiers, borrowing from operating
  system memory hierarchies to make a limited context window behave like a larger
  working memory.

## Claims & Evidence

### Letta's central abstraction is a stateful agent, not a stateless chat call.

The docs[^1] define stateful agents as agents that maintain memory and context across
conversations. The SDK[^3] creates and resumes durable agents and sessions.

Caveat/confidence: High.

### Letta persists more than visible context.

The stateful-agents guide[^4] says memories, messages, reasoning, and tool calls are
persisted in a database and remain retrievable after compaction or eviction.

Caveat/confidence: High for the API/SDK layer; exact storage implementation
depends on deployment.

### Memory blocks are for high-salience state that should always be in context.

The memory-block docs[^2] describe blocks as persistent context sections that are
always visible and need no retrieval.

Caveat/confidence: High.

### Block labels and descriptions guide autonomous memory edits.

The memory-block docs[^2] say descriptions are crucial because they tell the agent
how to use each block.

Caveat/confidence: High.

### Shared memory is a coordination primitive.

The shared-memory docs[^2] describe multiple agents attaching to the same block and
seeing updates immediately.

Caveat/confidence: High.

### Archival memory is retrieval-backed, not pinned context.

The archival-memory docs[^2] contrast it with memory blocks and describe
insert/search tools plus SDK passage endpoints.

Caveat/confidence: High.

### Letta uses a hierarchy rather than one memory store.

The context-hierarchy docs[^5] compare memory blocks, files, archival memory, and
external RAG by access pattern, tools, size, and count limits.

Caveat/confidence: High.

### Current Letta Agent stores memory as inspectable files.

The memory docs[^2] describe MemFS as a git-backed memory filesystem that projects
memory blocks into markdown files.

Caveat/confidence: High for Letta Agent; the V1 API docs still describe
database-backed concepts.

### Background reflection is part of the product model.

The memory docs[^2] describe dream subagents triggered by message count or
compaction, plus memory defragmentation flows.

Caveat/confidence: High.

### Letta's design lineage comes from MemGPT's virtual context management.

The MemGPT paper[^7] proposes OS-inspired memory tiers and control flow for
long-running conversations and large-document analysis.

Caveat/confidence: High.

## Important Terms

| Term                       | Meaning                                                                                                       |
| -------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Letta Agent                | A personalized, memory-first agent intended to persist across sessions and improve with use.                  |
| Stateful agent             | An agent whose memory, messages, tool calls, reasoning, and other state persist beyond one model invocation.  |
| Agent                      | The durable entity with memory, model configuration, message history, tools, and identity.                    |
| Conversation               | A single message thread or session with an agent; one agent can have many conversations.                      |
| Run                        | One invocation of an agent.                                                                                   |
| Step                       | One model-inference pass inside a run; long tasks can require many steps.                                     |
| Memory block               | A labeled, described, character-limited piece of core memory pinned into the context window.                  |
| Core memory                | Always-visible memory represented by attached memory blocks.                                                  |
| Shared block               | A memory block attached to more than one agent.                                                               |
| Archival memory            | Semantic long-term storage queried on demand through tools or SDK endpoints.                                  |
| Passage                    | An archival-memory item managed through agent passage endpoints.                                              |
| File                       | A larger read-only context object that can be opened, closed, grepped, or semantically searched.              |
| External RAG               | An outside retrieval system exposed to the agent through custom tools or MCP.                                 |
| MemFS                      | Letta Agent's git-backed memory filesystem for markdown-projected memory.                                     |
| Context repository         | Another name in the docs for the git-backed memory repository.                                                |
| `system/`                  | MemFS directory whose files are always loaded into the agent's system prompt.                                 |
| Dreaming                   | Sleep-time compute where background subagents consolidate recent experience into memory.                      |
| Memory defragmentation     | A cleanup flow that backs up memory and uses a subagent to split, merge, and reorganize memory files.         |
| Virtual context management | MemGPT's OS-inspired approach to moving information between fast in-context memory and slower external tiers. |

## Architecture Notes

Letta's memory model is easiest to read as four layers:

1. Always-loaded context: system prompt plus attached memory blocks, and in
   current Letta Agent, MemFS `system/` files.
2. Visible-but-not-loaded memory: file paths, descriptions, and memory tree
   metadata that let the agent decide what to open or reorganize.
3. Tool-retrieved stores: archival memory, files, and external retrieval systems
   queried by semantic search, grep, open/close, or custom tools.
4. Durable execution state: database-backed messages, reasoning, tool calls,
   runs, steps, and conversations that remain available after context eviction.

The practical design move is to stop treating the context window as the source
of truth. Context becomes a working set assembled from durable state. Memory
blocks keep critical facts hot, archival memory keeps lower-salience facts
searchable, files carry larger reference material, and background subagents
perform cleanup or consolidation when the foreground agent is not actively
answering.

## Memory Block Mechanics

A memory block has four important fields:

- `label`: the block's stable name, such as `persona`, `human`, `organization`,
  `scratchpad`, or `project`.
- `description`: the behavioral contract that tells the agent what belongs in
  the block.
- `value`: the actual memory content.
- `limit`: the block's character budget.

The docs[^2] present blocks as XML-like prompt sections. That matters because the
model does not see "memory" as an abstract database handle; it sees structured
text inside its active prompt. This makes blocks simple and inspectable, but it
also means block count and block size compete directly with the rest of the
context budget.

The recommended use cases are durable, high-value facts and behavioral
guidelines: user preferences, persona, project conventions, current task state,
tool guidelines, and shared policies. The wrong use case is bulk history or
large documents, because those should not be paid for on every turn.

## Archival Memory Mechanics

Archival memory is Letta's semantic long-term store. Agents can add entries with
`archival_memory_insert` and query them with `archival_memory_search`; developers
can manage the same class of data through passage endpoints.

Compared with memory blocks:

- It is not automatically placed in the context window.
- It scales to much larger stores.
- Retrieval depends on semantic search and tool use.
- It is better for lower-priority facts, conversation traces, support history,
  research notes, code examples, and references.
- Agent-side mutation is more append/search oriented; developer APIs have fuller
  management operations.

The tradeoff is the usual retrieval tradeoff: archival memory can hold more, but
the agent must decide to search, search well, and correctly use the results.

## Shared Memory and Multi-Agent State

Shared memory blocks let multiple agents read and update the same state. This is
not just a convenience API; it changes the coordination model. A supervisor can
hold one private persona block, a worker can hold another, and both can attach to
the same organization or policy block. Updating the shared block broadcasts
state without explicit agent-to-agent messaging.

This is useful for:

- Organization-wide policies.
- Multi-agent task state.
- Shared project facts.
- Supervisor/worker coordination.
- Central behavior changes that should affect many agents.

The risk is that shared writable memory becomes a global mutable dependency.
Treating some shared blocks as read-only policy, or giving them narrow
descriptions and size limits, keeps the coordination primitive from becoming a
dumping ground.

## Current Letta Agent Memory

The newer Letta Agent docs[^1] describe a more file-like memory experience than the
V1 SDK concept pages. Memory lives in MemFS, a git-backed memory filesystem. Each
projected memory file is markdown with YAML frontmatter; local agents store this
in local git repositories, while Constellation agents sync through Letta's cloud
and clone memory locally for editing.

Important operational details:

- `/init` bootstraps or refreshes memory by inspecting the current project and
  asking about working style when needed.
- `/remember` lets the user explicitly write durable instructions or facts.
- `/doctor` audits messy memory, placement, and token usage.
- `/sleeptime` configures background reflection.
- Dream subagents can run after a configured number of user messages or after
  compaction.
- Defragmentation backs up memory before a subagent reorganizes files, splits
  large files, or merges duplicates.
- Memory subagents use git worktrees so they can write memory concurrently
  without blocking the main agent.

The product implication is strong: memory is not only an API storage layer. It is
an editable, versioned workspace that both users and agents can inspect.

## Connections

- Related ideas: durable session logs, context compaction, retrieval-augmented
  generation, reflective memory, shared mutable state, git-backed state, memory
  as context assembly.
- Related sources: [sessions](/notebook/sessions/), [skills](/notebook/skills/),
  [agents-md](/notebook/agents-md/), [prompt-renderer-research](/notebook/prompt-renderer-research/).
- Contradictions or tensions: always-visible memory improves reliability but
  consumes prompt budget; retrieval memory scales but can be missed; shared
  memory simplifies coordination but introduces global mutable state; background
  dreaming can improve memory quality but adds another writer that must be
  audited.
- Useful applications: coding agents that learn repo conventions, personal
  assistants with durable preferences, customer-support agents with searchable
  histories, multi-agent systems with shared policy/state, and long-running
  research assistants that consolidate notes over time.

## Questions for Review

- What is the difference between an agent and a conversation in Letta?
  - An agent is the durable memory-bearing entity; a conversation is one thread
    with that agent.
- Why are memory blocks "core" memory?
  - They are attached to the agent and always visible in the context window.
- What field tells the agent how to use a memory block?
  - The `description` field.
- When should a fact go into archival memory instead of a memory block?
  - When it may be useful later but does not need to occupy context on every
    turn.
- How does shared memory coordinate agents?
  - Multiple agents attach to the same block, so updates by one become visible
    to the others.
- What does MemFS add beyond API-level memory blocks?
  - It projects memory into git-backed markdown files with versioning and local
    inspection/editing.
- What problem does dreaming try to solve?
  - It moves reflection and memory consolidation into background subagents rather
    than relying only on foreground turns.
- How does the MemGPT paper explain Letta's memory lineage?
  - It frames agent memory as virtual context management across fast and slow
    tiers, inspired by operating systems.

## Open Questions

- How exactly does current Letta Agent map MemFS markdown files back to API
  memory blocks, archival memory, or other internal state?
- Which memory edits are visible as durable audit events, and how are they
  associated with the foreground agent versus dream or doctor subagents?
- How does Letta resolve conflicts when multiple memory worktrees update related
  files at the same time?
- What heuristics decide whether a learned fact belongs in `system/`, another
  memory file, a memory block, or archival memory?
- How much developer control exists over forgetting, deletion, retention, and
  privacy policies?
- How should shared memory blocks be permissioned in multi-tenant systems?

## Takeaways

- Letta's key move is making the agent, not the conversation, the durable unit of
  state.
- Memory blocks are simple, powerful, and expensive: use them for important
  always-needed facts, not bulk history.
- Archival memory and files handle scale, but retrieval quality becomes part of
  correctness.
- Shared blocks make memory a multi-agent coordination primitive.
- MemFS turns memory into inspectable, versioned markdown, which makes agent
  learning easier to audit and edit.
- Dreaming moves memory improvement into background compute, but introduces more
  state mutation paths to reason about.

[^1]: https://docs.letta.com/letta-agent

[^2]: https://docs.letta.com/letta-agent/memory

[^3]: https://docs.letta.com/letta-agent-sdk/overview

[^4]: https://docs.letta.com/guides/core-concepts/stateful-agents

[^5]: https://docs.letta.com/guides/core-concepts/memory/

[^6]: https://github.com/letta-ai/letta

[^7]: https://arxiv.org/abs/2310.08560
