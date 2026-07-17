---
title: "Tau"
author: dpc; Hugging Face / Tau contributors
date: 2026-07-09
sources:
  - https://tau-agent.dev/
  - https://github.com/huggingface/tau
captured: 2026-07-09
tags: [tau, coding-agent, harness, prompts, agent-loop, unix, terminal-ui]
---

## Summary

Tau names two related coding-agent efforts: dpc's Tau emphasizes Unix-native process
composition, while Hugging Face Tau is a small Python teaching harness whose core lesson
is that a useful coding agent can be built from a provider-neutral event stream, a simple
tool-calling loop, explicit prompt assembly, and append-only sessions.

## Source Boundary

- **dpc's Tau (`tau-agent.dev`):** Unix-native, process-oriented harness.
  The public site focuses on architecture and product philosophy, not implementation details.
- **Hugging Face Tau (`huggingface/tau`):** Python implementation intended to be read.
  The repo provides the inspectable prompt builder, loop, tools, session wrapper, compaction
  code, and docs used for most of this note.
- **Shared lineage:** Both are inspired by Pi and prefer small harness layers, visible events,
  and terminal-native workflows.

## Key Ideas

- **Small layers instead of one harness blob:** Hugging Face Tau splits model-provider streaming
  (`tau_ai`), reusable agent state/loop/events (`tau_agent`), and the coding application
  (`tau_coding`). The core does not know about Textual, Rich, config paths, or local resource
  discovery.
- **Events are the contract:** Providers emit provider-specific chunks, Tau normalizes them, and
  frontends render provider-neutral agent events such as agent start/end, turn start/end, message
  deltas, thinking deltas, retries, tool start/end, queue updates, and errors.
- **The loop is deliberately plain:** Each turn streams a model response, appends the assistant
  message, executes requested tools, appends tool results, and repeats until the assistant returns
  no tool calls.
- **Prompts are assembled from runtime facts:** The system prompt is deterministic and includes
  identity, enabled tools, tool snippets, tool guidelines, optional custom/appended prompt text,
  project context, available skills, current date, and working directory.
- **Tools are ordinary typed executors:** Built-in coding tools are `read`, `write`, `edit`, and
  `bash`. They live in `tau_coding`, not the reusable agent package.
- **Sessions are append-only and replayed:** Tau stores durable session entries, reconstructs active
  messages from them, supports branching, and uses compaction entries to replace active context
  without rewriting the original history.
- **User input can be queued while work is running:** The harness has steering messages injected
  after the current turn/tool batch and follow-up messages injected when the agent would otherwise
  stop.
- **dpc's Tau pushes the same minimalism into Unix process boundaries:** Its site frames the harness
  as CLI -> harness over a Unix socket, with agents, tools, and extensions as standalone POSIX
  processes over stdio.

## What Tau Does

Hugging Face Tau is a terminal coding agent. Users ask it to inspect a project,
explain code, add tests, fix errors, or make changes. It can:

- read UTF-8 text files and supported image files;
- write new or replacement files;
- apply exact validated text edits;
- run shell commands in the session working directory;
- stream progress through an interactive Textual TUI or one-shot print mode;
- load project instructions from `AGENTS.md`, `.tau/`, and `.agents/` resources;
- load skills and prompt templates from user/project resource directories;
- track model/provider choice, thinking level, context usage, sessions, branches, and exports;
- compact old context manually or automatically.

dpc's Tau aims at a broader Unix-native agent environment. Its public site lists built-in
extensions for shell/filesystem work, web search, notifications, provider backends, and controlled
PIM access; roles and model knobs; multi-agent delegation; background tool work; review-friendly
diffs; approval logs; and filesystem/shell access locks.

## Prompt Surface

Tau's default system prompt is short and generated, not a large static prompt. In Hugging Face
Tau, `build_system_prompt()` builds it from `BuildSystemPromptOptions`.

The default prompt shape:

- identifies the assistant as an expert coding assistant inside Tau;
- says the assistant helps by reading files, executing commands, editing code, and writing files;
- renders an `Available tools` section from each enabled tool's `prompt_snippet`;
- adds guideline bullets collected from enabled tools and extra runtime guidelines;
- appends project context in XML-like `<project_context>` blocks;
- lists available skills in an XML-like `<available_skills>` block, but only when `read` is
  available so the model can load full skill files;
- ends with current date and current working directory.

The guideline collector is small but important:

- if `bash` is present without narrower exploration tools, tell the model to use bash for file
  operations such as `ls`, `rg`, and `find`;
- if separate exploration tools are present, prefer them over bash for file exploration;
- import each tool's prompt guidelines;
- add concise response and clear file path guidelines;
- de-duplicate everything deterministically.

Custom prompts replace the default identity/tools/guidelines section, but Tau still appends runtime
context: appended system prompt text, project context, skills, current date, and working directory.
That keeps user customization from accidentally removing operational facts.

## Tool Prompting

Tool definitions carry both provider schema and model-facing prompt metadata:

- `read`: file contents; prefer over `cat`/`sed`; supports offsets/limits and image metadata; head
  truncates large text.
- `write`: create or overwrite complete files; intended for new files or full rewrites.
- `edit`: exact replacement edits; every `oldText` must match once, edits must not overlap, and
  validation happens before writing.
- `bash`: run a shell command in `cwd`; combines stdout/stderr; supports timeout; tail truncates
  large output and records full output in a temp log.

The interesting design choice is that tool prompting is close to tool implementation. The system
prompt does not separately restate every behavior; it renders snippets and guidelines from the
actual enabled tools.

## Agent Loop

`run_agent_loop()` is the pure loop. It receives a provider, model, system prompt, mutable message
list, tools, optional max turn count, cancellation token, and queue callbacks.

One loop run:

1. Emit `AgentStartEvent`.
2. Validate `max_turns`.
3. Build a `tool_by_name` lookup.
4. For each turn, emit `TurnStartEvent`.
5. Ask the provider to stream a response using `model`, `system`, `messages`, and `tools`.
6. Convert provider stream events into agent events:
   - response start -> message start;
   - text delta -> message delta;
   - thinking delta -> thinking delta;
   - retry -> retry event;
   - response end -> append assistant message and emit message end;
   - provider error -> error event.
7. If no assistant message arrives, end the turn and stop with either cancellation, provider error,
   or a synthetic "provider stream ended without an assistant message" error.
8. If the assistant has no tool calls:
   - end the turn;
   - drain steering messages first;
   - if none, drain follow-up messages;
   - continue if either queue supplied messages, otherwise stop.
9. If the assistant requested tools:
   - execute tool calls sequentially;
   - emit tool start/end events;
   - append tool result messages;
   - end the turn;
   - drain steering messages;
   - continue.
10. If max turns are exhausted, emit a recoverable error.
11. Emit `AgentEndEvent`.

The loop is stateless except for mutating the caller-owned `messages` list. That keeps transcript
ownership in the harness/session layer while making the loop easy to test.

## Harness Behavior

`AgentHarness` is the reusable stateful brain around the pure loop. It owns:

- transcript messages;
- harness config;
- event listeners;
- cancellation token;
- running state;
- steering and follow-up queues.

`prompt(content)` appends a user message and starts the loop. `continue_()` runs the loop without a
new user message. If a prompt is already running, direct prompts are rejected; callers use
`steer()` or `follow_up()` instead.

The harness also repairs interrupted tool calls. Before a new run, it scans the transcript for
assistant tool calls that lack tool results and appends synthetic failed tool results. This matters
because OpenAI-compatible provider APIs reject inconsistent tool-call transcripts.

## Sessions & Compaction

`CodingSession` wraps the harness with the actual coding environment:

- default tools;
- project/user resources;
- provider configuration;
- credentials;
- slash command registry;
- durable JSONL storage;
- model and thinking-level entries;
- branch and leaf entries;
- session export;
- context accounting and compaction.

Compaction uses a separate summarization prompt, not the normal coding prompt. Its system prompt
instructs the model to summarize a conversation for future continuation and not continue the
conversation. The user prompt serializes messages in XML-like blocks, optionally includes a previous
summary, and asks for an exact structured format:

- Goal
- Constraints & Preferences
- Progress
- Key Decisions
- Next Steps
- Critical Context

Automatic compaction uses rough deterministic token estimates instead of provider tokenizers. The
default threshold is the model context window minus a reserve; unknown/custom models fall back to a
128k-token context window. Tau checks before a new prompt, after a successful turn, and after
context-overflow errors. On overflow, it can compact and retry once.

## dpc's Unix-Native Tau

dpc's Tau is useful as an architectural foil because it pushes the same "small parts" idea below
the language/runtime boundary:

- all major components are standalone POSIX processes;
- CLI talks to harness over a Unix socket;
- agent, tools, and extensions communicate over stdio;
- extensions are executables in any language;
- components can be independently sandboxed with OS tools such as containers, Landlock, jails, or
  bubblewrap;
- remote execution can be expressed as command prefixing, e.g. via SSH;
- packaging is meant to fit OS-native package managers;
- the product goal is terminal-native, scriptable, inspectable agent tooling.

This is stronger isolation philosophy than Hugging Face Tau's Python package boundaries, but it is
also a bigger system.

## Claims & Evidence

| Claim                                                                                                      | Support                                                                                                                    | Caveat / Confidence                                |
| ---------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Tau's core loop can stay small if UI, sessions, provider config, resources, and rendering live outside it. | `run_agent_loop()` only handles provider events, tool calls, queue drains, cancellation, and event emission.               | High for Hugging Face Tau.                         |
| Prompt assembly should be deterministic and data-driven.                                                   | `build_system_prompt()` formats enabled tools, tool guidelines, project context, skills, date, and cwd from typed options. | High.                                              |
| Tool guidance belongs with tool definitions.                                                               | `ToolDefinition` includes description, prompt snippet, prompt guidelines, input schema, and executor.                      | High.                                              |
| Event streams make multiple frontends easier.                                                              | Docs and code route print mode, Rich rendering, and Textual through provider-neutral agent events.                         | High.                                              |
| Append-only sessions allow compaction without erasing audit history.                                       | `CompactionEntry` replaces active replay context while original entries stay in the JSONL session file.                    | High.                                              |
| Queued steering and follow-ups improve live agent control.                                                 | `AgentHarness` has separate steering/follow-up queues, and the loop drains them at different stop points.                  | Medium-high; UX value depends on frontend clarity. |
| Unix process boundaries make extensions more inspectable and sandboxable.                                  | dpc's Tau site describes standalone POSIX components over sockets/stdio with per-component sandboxing.                     | Medium; public site claim, not code-reviewed here. |

## Important Terms

| Term              | Meaning                                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| `tau_ai`          | Provider layer that normalizes model APIs into Tau streams.                                       |
| `tau_agent`       | Reusable provider/tool agent core: messages, tools, events, loop, harness, session primitives.    |
| `tau_coding`      | Coding-agent application layer: CLI, TUI, file/shell tools, provider config, resources, sessions. |
| Agent loop        | Repeated model response -> tool execution -> tool-result feedback cycle.                          |
| Agent event       | Provider-neutral event consumed by frontends and renderers.                                       |
| Steering message  | Queued user input injected after the current turn or tool batch.                                  |
| Follow-up message | Queued user input injected when the current run would otherwise stop.                             |
| Compaction entry  | Durable session entry that replaces part of active replay context with a summary.                 |
| Prompt template   | Markdown prompt shortcut expanded before a turn.                                                  |
| Skill             | Markdown task instructions listed in the system prompt and loaded by the model when relevant.     |

## Lessons To Reuse

- Keep the core loop pure and boring: stream model events, append messages, run tools, append
  results, repeat.
- Use typed events as the boundary between agent core and UI.
- Keep prompt construction deterministic and visible; render runtime facts rather than hiding them.
- Put tool prompt snippets and operational rules next to the tool implementation.
- Treat skills as discoverable references, not automatically injected mega-context.
- Store sessions append-only; derive active context by replay.
- Model compaction as a session event so active context can shrink without mutating history.
- Add queue semantics explicitly if users can type while the agent is running.
- Separate "teaching implementation" simplicity from "production isolation" needs. Hugging Face Tau
  is readable; dpc's Tau is more serious about Unix process composition.

## Questions for Review

- Why does Tau keep the loop independent of CLI/TUI/session concerns?
  - So the same brain can drive multiple frontends and remain small enough to test directly.
- What makes Tau's system prompt maintainable?
  - It is assembled from typed runtime inputs and tool metadata instead of copied across frontend
    code paths.
- When does the loop stop?
  - It stops when the assistant returns no tool calls and there are no steering or follow-up
    messages to inject, or when cancellation/error/max-turn logic interrupts it.
- Why does Tau repair interrupted tool calls?
  - Provider APIs need every assistant tool call to have a corresponding tool result in submitted
    history.
- How does compaction preserve auditability?
  - The JSONL history remains append-only; only replayed active context substitutes summaries for
    older messages.

## Connections

- Related ideas: [Pi](/docs/notebook/pi/), [prompts](/docs/notebook/prompts/),
  [sessions](/docs/notebook/sessions/), [skills](/docs/notebook/skills/),
  [context-control](/docs/notebook/context-control/).
- Related sources: Pi's minimal tool loop and Goose's focused prompt fragments.
- Tension: Hugging Face Tau demonstrates simple Python package boundaries; dpc's Tau argues that a
  Unix-native agent should also move tools/extensions into separate processes.
- Conceptual use: build the first version around typed events, explicit prompt assembly, four narrow
  tools, append-only session records, and a direct loop before adding broader orchestration.

## Open Questions

- Which Tau should be treated as the primary reference for future implementation work?
  - **Recommendation**: Use Hugging Face Tau for code-level loop and prompt structure; use dpc's
    Tau for process/sandboxing architecture ideas.
- Should a small Rust harness copy Hugging Face Tau's queue semantics?
  - **Recommendation**: Yes, but only if the UI can clearly show pending steering versus follow-up
    messages.
- Should shell/file tools live in the reusable agent core?
  - **Recommendation**: No. Keep tool execution in the coding application layer so the core can stay
    provider/tool/event generic.
- Should compaction use provider-specific tokenizers?
  - **Recommendation**: Start with deterministic approximate accounting, then add provider-specific
    tokenizers only if estimates cause real failures.

## Takeaways

- Tau's loop is valuable because it is simple enough to explain completely.
- Prompt assembly is runtime data plus small defaults, not a giant static persona.
- The important architecture boundary is events and messages at the core, with tools, resources,
  sessions, and UI wrapped outside.
