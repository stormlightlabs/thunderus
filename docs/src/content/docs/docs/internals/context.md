---
title: "Context Assembly"
---

Context assembly turns workspace files, configured skills, session state, and
the current prompt into the model-visible request. The application discovers
filesystem inputs and owns persistence.

`thndrs-agent` owns deterministic selection, budgeting, lifecycle, and reduction
policy. `core/prompt` renders the selected result into provider messages.

## Mental Model

```text
workspace root
   │
   ├── AGENTS.md inventory ──┐
   ├── skill metadata ───────┤
   ├── transcript/session ───┤── refresh_context_ledger ───┐
   └── pins and summaries ───┘                             │
                                                           ▼
                                                   ContextLedger
                                                           │
                                          PromptBundle + tool catalog
                                                           │
                                                           ▼
                                             provider message projection
```

A turn does not send the entire UI transcript or every discovered file. The
ledger assigns typed candidates visibility, lifecycle, protection, relations,
and a budget. The prompt renderer uses that projection and keeps the complete
application transcript available for the UI and session records.

## Responsibilities

The application owns filesystem and session adapters:

- `core/context` finds the workspace, reads `AGENTS.md`, selects applicable
  instruction sources, and reports discovery diagnostics.
- `core/skills` discovers skill metadata and loads full Markdown only when a
  skill is activated or explicitly read.
- `cli/app/context` converts harness, instruction, skill, transcript, pin,
  permission, and compaction inputs into `SelectionInput`, invokes the library
  selector, merges durable lifecycle state, and stores the current ledger.
- `core/prompt` assembles named prompt fragments, selected project context,
  skill metadata, tool schemas, and model-visible transcript entries.
- `thndrs-agent::context` selects and reduces candidates without knowing the
  filesystem, terminal, provider payloads, or session records.

The application refreshes the ledger at turn boundaries and after lifecycle
changes such as tool results, permission changes, compaction, failure, and
cancellation. The built-in provider path attaches the resulting ledger to its
`PromptBundle`. ACP runs receive their ACP-specific prompt and configuration
through the ACP runner rather than the built-in provider message lowering path.

## Workspace Discovery

`discover_workspace_root` runs `git rev-parse --show-toplevel` from the CLI
working directory. If that fails, it uses the canonicalized working directory.
The root is used for instruction discovery, skill discovery, prompt metadata,
relative tool paths, and the default session directory.

`discover_instructions` loads the root `AGENTS.md`, then recursively looks for
nested `AGENTS.md` files up to `MAX_DISCOVERY_DEPTH`. It skips hidden, VCS, and
common build directories such as `target`, `node_modules`, `dist`, `build`, and
`out`. Sources carry an absolute path, relative scope, content hash, byte count,
and capped content. Files larger than `AGENTS_MD_SIZE_CAP` are truncated and
produce a warning diagnostic.

The inventory is loaded at application startup. A turn-boundary snapshot can
compare path and content hashes and report files added, removed, or changed
since the previous turn. The selected source metadata and diagnostics are also
available to the context ledger and session audit.

Skills use the same workspace root plus configured skill directories. Default
roots include the project compatibility locations under `.thndrs/skills`,
`.agents/skills`, `.claude/skills`, `.codex/skills`, `.pi/skills`, and
`.pi/agent/skills`. User and configured roots are also considered. Discovery
reads frontmatter metadata from `SKILL.md`, applies depth and metadata limits,
and reports invalid or unreadable skills as diagnostics. It does not load every
skill body into the initial prompt.

## Instruction Precedence

Instruction files are project context, not a permission grant. The application
loads the root source and nested sources in scope order. `select_instructions`
marks the root as applicable and can include a nested source when mentioned or
pinned paths fall under that source's relative directory. The closest
applicable scope is ordered first. The current ledger refresh supplies pinned
paths. Callers that have path mentions can supply those too. Discovered sources
outside the referenced scopes remain as metadata for inspection rather than
being rendered as active instructions.

The model-facing prompt places application policy before project context. Its
system message contains, in order:

1. fixed `thndrs` identity and communication fragments
2. action, edit, safety, self-knowledge, and web-source guidance
3. workspace, model, and date metadata
4. the internal self-knowledge snapshot
5. either the selected context projection or the legacy `AGENTS.md` block
6. available skill metadata

The current user turn is added as the final provider message after the selected
transcript tail. Project instructions therefore cannot replace the harness's
application policy, tool authority, or user input handling. The prompt may
include a size-capped file body, but the path, scope, hash, and truncation state
remain explicit.

When a context ledger is attached, the renderer emits `<selected_context>`
items for project instructions, summaries, pinned handles, and skill metadata.
The legacy path renders `<project_context>` when no ledger is attached. Provider
history reuse can omit unchanged root instruction text only when that provider
explicitly supports reusable history. Built-in providers default to including
the active capped content.

## Skill Selection

Startup exposes compact `SkillMetadata`—name, description, path, source, and
allowed tools—to the prompt as routing information. The model can identify a
skill without receiving all of its instructions.

The user can browse and activate a discovered skill from the skills picker.
Activation calls `skills::load_skill`, validates and bounds the skill package,
loads its Markdown and permitted references, appends an `Entry::Skill`, and
writes a skill activation record when session persistence is enabled. An
activated skill becomes model-visible as assistant-context content, and its root
is added to the agent's extra read roots. A tool can also activate a skill
through the application lifecycle path without treating discovery metadata as
full instructions.

Skill content is still context, not authority. The built-in tool registry and
runtime authority decide which operations the agent can perform. Skill
frontmatter can describe allowed tools for routing and diagnostics, but it does
not bypass those checks.

## Conversation Context

The full transcript serves the UI and session writer.

Prompt projection is smaller:

- the ordinary projection considers the latest 20 transcript positions
- only finalized user, assistant, reasoning, and completed tool entries are
  eligible. Status, errors, permissions, and running entries are UI state
- activated skills with content remain available even after they move beyond
  the ordinary tail
- a ledger can replace the fixed tail with its selected transcript items,
  summaries, pins, and protected evidence handles.

`refresh_context_ledger` creates transcript candidates with sequence numbers,
labels, byte estimates, streaming state, protection, and artifact handles. It
also supplies the pending user turn, pending permissions, compaction summaries,
dropped ids, and durable context lifecycle records. The selector applies model
limits and context policy, then the app carries forward lifecycle state such as
protection, duplicates, supersession, and verification relations.

Tool output is projected separately from the UI display. The application may
retain bounded evidence behind an artifact handle while sending a reduced or
model-facing result. This keeps the request useful without making the full
filesystem or process output part of the provider prompt by accident.

Automatic compaction uses the same ledger budget. Before a built-in run,
`preflight_requires_auto_compaction` estimates the lowered provider messages
and applies the configured model limit and compaction policy. A successful
summary becomes a selected context item. A failed or rejected compaction leaves
the current projection and pending user turn intact.

## Tool Exposure

`runtime_tool_definitions` builds the catalog for the current runtime, including
built-in tools and available MCP tools. The catalog is attached to the
`PromptBundle` and rendered into provider-native tool schemas. The provider run
rebuilds the runtime definitions for dispatch, so the schema and executor share
the same registry boundary.

Tool visibility does not equal permission. Authority, permission prompts,
workspace containment, process handling, and MCP connection policy are enforced
at dispatch time. Tool calls and their results are represented as normalized
agent events and application audits. Raw provider tool payloads stay inside the
provider adapter and active run.

## Provider Projection

`PromptBundle` is the application-side intermediate representation. It contains
named prompt fragments, environment metadata, selected project context, skill
metadata, tool definitions, projected transcript entries, the current user turn,
and optional ledger state.

`render_system_prompt` combines the fixed fragments, environment, self-knowledge,
and selected context. `lower_to_provider_messages` emits a provider-neutral
message list: the rendered system content is the first user message, finalized
transcript entries follow in role order, and the current prompt is last. Tool
results become user messages in the projection. Finalized assistant and
reasoning entries become assistant messages. Each provider adapter then converts
that list and the tool schemas into its own request format.

The public `thndrs-agent` crate receives provider-neutral messages and typed
context/accounting data. It does not receive `AGENTS.md` paths, application
session records, Ratatui state, or provider wire payloads.

## Boundaries

- `core/context` owns discovery and instruction-file adapters but does not
  decide provider wire formats.
- `core/skills` owns skill inventory and bounded loading but does not grant
  tool authority.
- `cli/app/context` owns application candidate construction, ledger retention,
  durable lifecycle merging, and context commands.
- `thndrs-agent::context` owns pure selection, budgeting, compaction, reduction,
  and lifecycle policy.
- `core/prompt` owns fragment ordering and lowering to provider-neutral
  messages. Provider modules own the final wire conversion.
- `core/tools` and `core/mcp` own catalog and execution adapters. Context
  assembly describes tools but does not execute them.
- `core/session` stores configured context metadata, snapshots, activations, and
  audits according to the capture policy. It is not the source of model
  projection during a live turn.

## Key Types

- `ContextSource`, `InstructionInventory`, `InstructionSelection`, and
  `InstructionSnapshot` — discovered instruction files and turn-boundary change
  detection.
- `SkillMetadata`, `SkillInventory`, and `LoadedSkill` — skill routing metadata
  and on-demand content loading.
- `SelectionInput`, `ContextLedger`, `ContextItem`, and `ContextProjection` —
  candidate selection and model-visible context state.
- `ContextLifecycle`, `ContextProtection`, and `ContextRelation` — durable
  protection, deduplication, supersession, and verification state.
- `PromptBundle`, `PromptFragment`, and `EnvironmentMetadata` — structured
  prompt assembly before provider lowering.
- `ProviderMessage` — provider-neutral lowered message content.
- `ToolDefinition` and the runtime tool catalog — model-visible tool schemas.
- `ContextSnapshot` and `ContextLedgerMeta` — session-facing context audit
  records.

## Invariants

- Workspace discovery establishes one root for instructions, skills, tools,
  prompt metadata, and default session paths.
- Instruction and skill discovery is bounded. Oversized or unreadable inputs
  produce diagnostics rather than unbounded prompt content.
- Project instructions remain below application policy in prompt assembly and
  do not grant permissions.
- Full skill Markdown is loaded on activation, not for every discovered skill.
- Only selected, finalized, model-visible transcript entries enter provider
  messages. The UI transcript remains a separate, richer projection.
- A ledger's budget and model limits govern transcript, instruction, skill, and
  summary selection. The provider request is built from the resulting
  projection, not from the unfiltered application state.
- Tool schemas describe available dispatch entries, while authority and
  permission checks remain at execution time.
- Provider wire payloads do not appear in public library APIs.

## Source Map

| Responsibility                               | Primary source                                                    |
| -------------------------------------------- | ----------------------------------------------------------------- |
| Workspace root and root instruction loading  | `crates/thndrs/src/core/context/mod.rs`                           |
| Nested instruction discovery and selection   | `crates/thndrs/src/core/context/instructions.rs`                  |
| Skill roots, metadata discovery, and loading | `crates/thndrs/src/core/skills.rs`                                |
| Startup inventory construction               | `crates/thndrs/src/cli/app.rs:App::build`                         |
| Ledger candidate construction and refresh    | `crates/thndrs/src/cli/app/context.rs:refresh_context_ledger`     |
| Context selection and lifecycle policy       | `crates/thndrs-agent/src/context/`                                |
| Prompt fragments and bundle                  | `crates/thndrs/src/core/prompt/mod.rs`                            |
| Provider message lowering                    | `crates/thndrs/src/core/prompt/mod.rs:lower_to_provider_messages` |
| Runtime prompt and tool catalog assembly     | `crates/thndrs/src/runtime/interactive.rs:spawn_agent`            |
| Tool definitions and schemas                 | `crates/thndrs/src/core/tools.rs` and `tools/registry.rs`         |
| Context metadata and snapshots               | `crates/thndrs/src/core/session/contracts.rs`                     |
| Context lifecycle event handling             | `crates/thndrs/src/cli/app/agent_lifecycle.rs`                    |

## Related

- [Codebase tour](/docs/internals/codebase/)
- [Runtime and state](/docs/internals/runtime/)
- [Request lifecycle](/docs/internals/lifecycle/)
- [Providers](/docs/internals/providers/)
- [Tools](/docs/internals/tools/)
- [Sessions](/docs/internals/sessions/)
- [Adding a provider](/docs/development/adding-a-provider/)
