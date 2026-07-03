---
title: "Context Control and Memory System Design"
Author: Pi, Letta, Polytoken, AGENTS.md, Memory Sandbox, VISTA, OpenAI, Git
Date: 2026-07-03
Captured: 2026-07-03
Tags: [context-control, memory, coding-agent, prompt-assembly, context-ledger]
Sources:
  - https://pi.dev/docs
  - https://mariozechner.at/posts/2025-11-30-pi-coding-agent/
  - https://github.com/earendil-works/pi
  - https://docs.letta.com/letta-agent/memory
  - https://docs.letta.com/guides/core-concepts/memory/
  - https://agents.md/
  - https://developers.openai.com/cookbook/examples/how_to_count_tokens_with_tiktoken
  - https://git-scm.com/docs/gitignore
  - https://www.sqlite.org/fts5.html
  - https://arxiv.org/abs/2308.01542
  - https://arxiv.org/abs/2603.07670
  - https://arxiv.org/abs/2606.30005
---

## Summary

A strong context-control system treats model context as an inspectable,
user-controlled working set assembled from durable session state, scoped
project instructions, explicit pins, summaries, and memory stores; memory helps
only when users can see, edit, delete, and audit what the agent remembers.

## Key Ideas

- **Context control is more than token trimming:** Pi's useful lesson is that
  hidden prompt, tool, and context injection make behavior harder to predict.
  The harness should show what was loaded and keep context sources explicit.
- **Memory must be user-visible:** Memory Sandbox argues that users need
  affordances to view and control what an agent remembers, otherwise they form a
  poor mental model of why the agent behaves as it does.
- **The model needs context-state signals:** VISTA frames agents as partly
  blind to their own context. A dashboard with block id, type, token cost,
  recency, status, and recovery handle lets the model make better keep/drop
  decisions than raw transcript text alone.
- **Durable history is not active context:** Polytoken and other local-first
  agents separate the full session event record from the bounded subset sent to
  the model.
- **Letta's hierarchy is the right product shape:** Small critical facts belong
  in always-visible memory, large references should be searchable/openable,
  lower-priority facts belong in archival memory, and consolidation should be
  visible as a memory operation rather than an invisible prompt mutation.
- **AGENTS.md needs scoping and restraint:** The public AGENTS.md guidance
  recommends nested files where the closest file wins and says explicit chat
  prompts override file instructions. Recent research also warns that bloated
  context files can reduce task success and increase cost, so project context
  should stay scoped, minimal, and auditable.
- **Progressive disclosure generalizes beyond skills:** A robust context system
  discovers metadata first, loads full text only when selected, and records what
  entered the prompt.

## Claims & Evidence

### Visible Context Is A Product Requirement For Predictable Agents

Pi notes emphasize small prompts and surfaced context. Memory Sandbox identifies
user control over memory as the missing affordance for understandable agent
behavior.

Caveat/confidence: High.

### A Memory System Should Start With Inspectable Records

Letta's MemFS and memory commands make memory inspectable. Memory Sandbox treats
memories as user-manipulable data objects, which supports a visible write/edit
model over autonomous hidden writes.

Caveat/confidence: High.

### A Context Dashboard Can Improve Model-Side Management

VISTA reports gains from typed addressable blocks plus token, recency, access,
status, and recovery metadata. This supports giving the model a compact view of
context state rather than only raw text.

Caveat/confidence: Medium-high; the research setting may not map directly to
every coding agent.

### Summaries Must Not Be The Only Remaining Evidence

VISTA distinguishes summaries from recoverable byte-identical archives.
Session-oriented agents also preserve durable event history, which supports
treating compaction as active-context reduction rather than deletion.

Caveat/confidence: High.

### Long-Lived Memory Requires Write Filtering And Governance

The 2026 memory survey calls out write-path filtering, contradiction handling,
latency budgets, and privacy governance as core long-term memory concerns.

Caveat/confidence: High.

### Scoped Project Instructions Are Safer Than One Large Instruction Blob

AGENTS.md recommends nested files and closest-file precedence. Empirical
research warns that overbroad context can hurt task success and cost.

Caveat/confidence: High.

### Exact Token Counting Is Provider- And Model-Specific

OpenAI's tokenizer guidance says different models use different encodings and
recommends model-aware tokenizer libraries for exact counts.

Caveat/confidence: High.

### Repository-Shared Memory And User-Local Memory Need Different Storage Expectations

Git's ignore documentation distinguishes repository-shared `.gitignore`,
repository-local `.git/info/exclude`, and user-global `core.excludesFile`.

Caveat/confidence: High.

### SQLite FTS5/BM25 Is A Good First Local Retrieval Index For Memory

SQLite FTS5 supports local full-text search, relevance ranking, snippets,
prefix, phrase, NEAR queries, and external-content indexing.

Caveat/confidence: High.

## Important Terms

| Term              | Meaning                                                                                                                                            |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Context ledger    | Per-turn inventory of candidate and selected context items, with ids, kind, source, byte count, token estimate, inclusion status, and diagnostics. |
| Working set       | The bounded set of context items rendered into the next model request.                                                                             |
| Context dashboard | Model-visible and user-visible summary of the working set and omitted/recoverable items.                                                           |
| Core memory       | Small high-salience memory that is usually included, such as user preferences or durable project conventions.                                      |
| Project memory    | Workspace-local remembered facts intended to apply within one repository or project.                                                               |
| User memory       | User-local remembered facts intended to apply across projects.                                                                                     |
| Archival memory   | Larger or lower-salience memories that are searchable and recoverable but not loaded by default.                                                   |
| Pin               | Explicit instruction that a context item should remain in the working set until removed or expired.                                                |
| Compaction        | A durable summary record that lets later turns omit older transcript detail while preserving the full session log.                                 |
| Recovery handle   | Stable id/path/hash that lets omitted context be reopened without losing exact evidence.                                                           |

## Design Lessons

### Context Must Become Addressable

Raw prompt strings do not give users or models enough leverage. Each meaningful
piece of context should have an id and type: root instruction file, nested
instruction file, skill metadata, loaded skill content, pinned file excerpt,
transcript segment, compaction summary, user memory, project memory, or tool
result archive.

Addressability enables commands such as `context show`, `pin`, `drop`,
`recover`, and `doctor`. It also lets the session log record decisions without
storing sensitive content.

### Memory Should Be File-Backed First

Letta's MemFS direction is stronger than a black-box vector store as the first
user-facing contract. Markdown files with frontmatter are easy to inspect,
diff, edit, delete, back up, and include in project review.

A practical first shape:

- user-local core memory for small durable preferences;
- user-local archival notes for cross-project facts;
- project-local core memory for repository conventions;
- project-local archival notes for project facts.

Autonomous memory writes should begin as suggestions or confirmed operations.
The reliable base is explicit remember/edit/delete commands plus session
records for every memory write.

### Context Control Needs Both User And Model Surfaces

The user needs a command or UI view that answers "what does the agent know right
now?" The model needs a compact dashboard that answers "what is visible, what
is expensive, what is pinned, and what can be recovered?"

The dashboard should not include hidden evidence. It should include metadata:
ids, labels, kinds, token estimates, recency, inclusion state, and recovery
paths. Full content still flows through normal prompt sections or file tools.

### Compaction Must Be Reversible Enough

Session compaction should write a summary event, but the original records must
remain. Later turns can include the summary instead of old transcript entries,
while inspect/replay can still recover the original tool outputs and assistant
messages.

This is the difference between "summarized away" and "summarized for the active
working set."

## Resolved Design Questions

### What Token Estimator Should Be Used Before Exact Provider Tokenizers Are Integrated?

Use a conservative byte/character heuristic only as a budget guard, label it
approximate, and leave room for provider-specific tokenizers. OpenAI's
tokenizer guidance says model encodings differ, and exact counts come from
encoding the text with the model's tokenizer.

### Should User Memory Be Global By Default, Or Should Every Memory Item Require A Scope?

Require an explicit scope field. Memory Sandbox's user-control framing and the
memory survey's governance concerns both argue against unscoped persistence.
Global user memory can exist, but it should be labeled as such.

### Should Project Memory Be Committed Or Ignored By Default?

Do not force one behavior. Git's own ignore model distinguishes shared
repository policy from local per-repository and user-global exclusions. Project
memory that all collaborators should share can be committed; personal project
memory belongs in a local exclude path or user memory.

### How Should Stale Memory Conflicts Be Displayed?

Surface conflicts as diagnostics with source, scope, timestamp/hash, and the
competing item. AGENTS.md precedence says explicit prompts outrank file
instructions and closest files win; memory should be lower than direct
instructions and visible when it conflicts.

### What UI Makes Memory Deletion Practical?

Treat memories as data objects with list, detail, edit, and delete affordances.
Memory Sandbox directly supports this: users need to view and manipulate
remembered objects, not merely hope the agent forgets.

### When Should Autonomous Memory Suggestions Become Acceptable?

After explicit memory CRUD, source metadata, conflict diagnostics, and audit
records exist. The memory survey highlights write-path filtering and
trustworthy reflection as hard problems, so autonomous writes should start as
user-confirmed suggestions.

### Should Archival Memory Use Embeddings Immediately?

No. Start with inspectable files plus a rebuildable SQLite metadata and
FTS5/BM25 index. That gives local exact-term retrieval for commands, paths,
package names, errors, tags, and headings while preserving Markdown as the
source of truth. Embeddings can be added later as another derived index over the
same files if semantic recall becomes the bottleneck.

### Should Compaction Delete Old Context?

No. Research systems such as VISTA distinguish summaries from recoverable exact
payloads. Compaction should reduce the active working set while preserving
durable evidence.

## Implementation Pattern

A general context-control loop has eight stages:

1. Discover candidate context sources.
2. Index file-backed memory into a rebuildable metadata plus FTS5/BM25 cache.
3. Estimate size and classify trust/source/scope.
4. Select a working set under budget.
5. Render selected items into the model request.
6. Render a compact context dashboard.
7. Persist a context ledger record for audit.
8. Expose commands to inspect, pin, drop, remember, compact, and recover.

This pattern keeps memory as part of context assembly, not a separate invisible
retrieval channel.

## Questions For Review

- Why should memory be built on top of context control?
  - Because memory only helps if the user and model can see when it is loaded,
    why it was selected, and how to correct it.
- What should be automatic first?
  - Discovery, metadata, budgeting, session audit, and safe default selection.
    Durable memory writes should begin as explicit operations.
- What makes a memory item safe to keep?
  - It has a source, scope, owner, timestamp, hash, review/edit path, and a
    deletion path; it is not a secret and does not override higher-priority
    instructions.
- When should a fact be pinned instead of remembered?
  - Pin task-local evidence that should stay visible during the current work;
    remember durable preferences, conventions, or lessons likely to matter
    across sessions.

## Connections

- Related ideas: [Letta memory](/notebook/letta/), [Pi](/notebook/pi/),
  [Polytoken](/notebook/polytoken/), [sessions](/notebook/sessions/),
  [AGENTS.md](/notebook/agents-md/), [skills](/notebook/skills/), [prompts](/notebook/prompts/).
- Related sources: Memory Sandbox, VISTA, the 2026 memory survey, AGENTS.md
  public guidance, OpenAI tokenizer guidance, Git ignore documentation.
- Contradictions or tensions: always-loaded memory improves continuity but can
  create stale or overbroad instructions; autonomous memory writes can improve
  recall but risk surprising the user; summaries save tokens but can erase
  evidence unless the original remains recoverable.
- Useful applications: context ledger, context dashboard, explicit memory
  writes, compaction, scoped project instructions, memory file store, context
  doctor.

## Takeaways

- Build memory as inspectable context infrastructure, not as hidden retrieval.
- Start with explicit file-backed user/project memory, pins, compaction, and a
  context ledger before adding autonomous consolidation.
- Give both the user and the model a dashboard of context state: what is
  visible, what is omitted, what is pinned, what is expensive, and what can be
  recovered.
