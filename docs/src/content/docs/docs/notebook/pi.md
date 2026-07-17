---
title: "Pi"
Author: Mario Zechner / Earendil Works
Date: 2026-06-28
Captured: 2026-06-29
Tags: [coding-agent, harness, terminal-ui, agent-loop, minimalism]
---

## Summary

Pi argues that a coding harness can stay small and inspectable by exposing simple tools,
explicit context, event-streamed agent state, file-based planning, and terminal-native
workflows instead of baking in heavyweight modes, hidden orchestration, or in-process
permission theater[^pi-article].

## Key Ideas

- **Minimal core, extensible edges:** Pi splits LLM API, agent loop, TUI, and CLI harness
  into separate packages. The CLI adds sessions, tools, themes, context files, and customization.[^pi-github]
- **Context control matters:** The article stresses that hidden prompt/tool/context
  injection makes model behavior harder to predict.
  Pi keeps prompt/tool definitions small and surfaces what gets loaded.[^pi-article]
- **Four default tools can be enough:** Pi's default coding surface is read, write, edit,
  and bash, with separate read-only tools available for restricted runs.[^pi-docs]
- **Events are the UI contract:** `pi-agent-core` emits agent, turn, message, stream
  update, and tool execution events; a UI can subscribe and render incrementally.[^pi-github]
- **Queueing is a first-class interaction:** Pi supports steering messages during work
  and follow-up messages after work, rather than forcing the user to wait silently.[^pi-docs]
- **Use the terminal instead of rebuilding it:** The Pi article chooses an
  append-to-scrollback TUI for coding-agent chat so native terminal search/scrolling keep
  working.[^pi-article]
- **No built-in plan mode/todos/sub-agents/MCP by default:** Pi prefers files, CLI tools
  with READMEs, tmux, and explicit separate sessions over hidden state and heavyweight tool surfaces.[^pi-article]
- **Sandbox outside the harness:** Pi does not pretend permission prompts solve the core
  security problem; its docs recommend containers, micro-VM routing, or policy sandboxes when stronger boundaries are needed.[^pi-docs]
- **Shell execution is a normal local capability:** Pi's built-in `bash` tool spawns the
  configured shell in the working directory, streams combined stdout/stderr, supports timeout and
  abort, kills the process tree, truncates visible output, and saves oversized output to a temp file.
  It does not classify commands for approval.[^pi-github]
- **Project trust is input loading, not command permission:** Pi asks/tracks whether project-local
  settings, extensions, skills, prompts, and themes may be loaded. Once running, built-in tools and
  extensions use the permissions of the `pi` process.
- **Prompt guardrails stay small:** Pi's default system prompt emphasizes the available tools,
  concise answers, clear file paths, project context, skills, date, and working directory. Tool
  descriptions carry operational details such as bash truncation and timeout behavior.

## Claims & Evidence

| Claim                                                                           | Support                                                                                                                             | Caveat / Confidence                                                                                       |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| A useful coding agent does not need a huge system prompt.                       | Pi reports its prompt plus tool definitions are under 1000 tokens and relies on project `AGENTS.md` for customization.              | Medium; this is the author's experience and benchmark framing, not a universal proof.                     |
| Agent runtime should expose event flow instead of hiding it.                    | `pi-agent-core` documents `agent_start`, `turn_start`, `message_start/update/end`, `tool_execution_*`, `turn_end`, and `agent_end`. | High; event streams map cleanly to any UI.                                                                |
| Tool result data should separate model content from UI detail.                  | The article calls out separate tool result blocks for LLM content and UI rendering detail as a useful abstraction.                  | High; keep model-visible content distinct from transcript decoration.                                     |
| Native scrollback is a good fit for linear coding-agent chats.                  | The article argues coding agents are mostly linear chat plus tool output, so terminal scrolling/search are valuable.                | Medium-high; products with dashboards or panes may still choose full-screen ownership.                    |
| Built-in background process management can be avoided until it has clear value. | Pi recommends tmux for long-running servers/debuggers and keeping bash synchronous.                                                 | Medium-high; long-running supervised tasks should be a deliberate feature, not an incidental side effect. |
| Plan mode and todos can be files.                                               | Pi recommends `PLAN.md`/TODO files for persistent, visible planning state.                                                          | High; this aligns with this repo's simplicity rule.                                                       |
| Permission prompts are not a reliable security boundary for shell commands.     | Pi documents no built-in sandbox and says real isolation should come from OS, VM, container, or sandbox boundaries.                 | High; this should shape docs and UX language even if `thndrs` keeps narrower tools.                       |

## Important Terms

| Term                     | Meaning                                                                                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent loop               | The repeated process of sending context to a model, streaming a response, executing tool calls, feeding results back, and stopping when no tool calls remain. |
| Steering message         | User input queued while the agent is working, delivered after the current turn/tool batch.                                                                    |
| Follow-up message        | User input queued to run after the agent finishes current work.                                                                                               |
| Context file             | Project/user instructions loaded into the agent context, such as `AGENTS.md`.                                                                                 |
| Tool preflight           | A hook before tool execution that can validate or block a call.                                                                                               |
| Append-to-scrollback TUI | TUI model that writes mostly linearly to terminal scrollback and only redraws a small active region.                                                          |
| Project trust            | A decision about whether to load project-local resources. It does not sandbox later tool calls.                                                               |

## Command Execution Review

Pi's shell execution path is deliberately simple:

- `packages/coding-agent/src/core/tools/bash.ts` defines one model-facing `bash` tool with
  `command` and optional `timeout`.
- The local backend resolves bash, spawns it with `-c` or stdin transport, runs in the selected
  working directory, merges stdout/stderr into one streamed output path, and uses an abort signal
  or timeout to kill the process tree.
- Output is accumulated through a bounded buffer. Large output is truncated for model/UI display
  and the full output is persisted to a temp log when needed.
- Exit codes are returned through normal tool success/error semantics: non-zero exit is an error
  with captured output plus the exit code appended.
- Hooks/extensions can wrap or replace execution. The core implementation does not provide a
  built-in approval system for network, destructive commands, or writes outside the project.

Security is documented as an environment concern rather than a shell parser concern:

- Pi runs with the permissions of the local user account.
- Built-in tools can read, write, edit, and execute with that account's permissions.
- Extensions are local TypeScript modules with the same trust boundary.
- Project trust prevents unapproved project resources from being loaded; it does not restrict the
  model's later tool use.
- Untrusted or unattended work should run inside a container, VM, micro-VM, remote sandbox, or
  policy-controlled sandbox with minimal mounted files and credentials.

The useful lesson: do not add command-permission classifiers unless they are backed by
a real process boundary. Prefer transparent local execution, narrow first-party tools, transcripted
audit data, clear docs, and prompt instructions that steer the model toward restraint.

## Prompt Guardrails To Reuse

Useful `pi` prompt-level guardrails:

- State that the assistant is operating inside the harness and can read files, run commands, edit
  code, and write files only through available tools.
- Keep the available tool list visible and concise.
- Add a small guideline to use narrower tools for file search, reads, edits, and URL reads before
  shell when they fit.
- Add a small guideline to avoid destructive commands unless the user explicitly requested them or
  they are clearly necessary and scoped.
- Show the current date and working directory at the end of the prompt.
- Load project context in labeled blocks and treat it as guidance below direct user/system
  instructions.
- Put operational details in tool descriptions: working directory, timeout, cancellation, output
  truncation, and transcript/audit behavior.

## Conceptual Lessons

- Keep the harness local and inspectable: no hidden planner, no hidden sub-agent,
  no MCP surface.
- Keep a typed event stream for user, assistant, reasoning, tool, error, and
  completion states.
- Represent tools as explicit Rust structs/functions with clear input/output;
  defer broad provider abstraction until more than one provider truly exists.
- Store sessions as append-only records when audit/resume matters.
- Prefer CLI/file workflows for planning and context gathering. If users want a plan,
  write/read a Markdown file.
- Do not build permission theater into the UI. If safety matters, design a real sandbox
  boundary later.
- Shell/process docs should say commands run as the local user. The harness should provide
  cancellation, timeout, output caps, transcript audit, and optional external sandbox integration,
  not pretend command parsing is a security boundary.
- Keep command guardrails mostly model-facing and tool-facing: prefer narrow tools, avoid
  unnecessary destructive commands, expose cwd/output/status clearly, and document that untrusted
  work belongs in a container/VM/sandbox.
- Treat native scrollback versus full-screen ownership as a UX decision, not an
  automatic consequence of the terminal UI library.

## Questions for Review

- Should a Rust harness preserve native scrollback like Pi, or use a full-screen
  model for richer panes and dashboards?
  - Preserve native scrollback for chat-first workflows and choose full-screen ownership only when panes or dashboards become central.
- What is the minimum event enum that supports streaming model text and tool calls?
  - Start with typed events for user input, assistant/reasoning deltas, tool start/output/end, errors, cancellation, and completion.
- When should project context expand beyond `AGENTS.md` into README files,
  selected snippets, or explicit user attachments?
  - Expand context only when the user asks for it or when task-local evidence shows AGENTS.md is insufficient.

## Connections

- Related ideas: Ratatui TEA gives the Rust UI state model;
  Gridland gives the target layout;
  Pi gives harness philosophy and agent event semantics.
- Related sources: Pi README package split, `pi-agent-core` event flow, `pi-tui`
  retained-mode rendering, Pi containerization docs.
- Contradictions or tensions: Pi's TUI is append-to-scrollback and TypeScript; this
  project targets Rust + Ratatui, which normally redraws a full viewport.
- Conceptual use: build a local coding harness whose complexity lives in
  explicit event/data types, not hidden modes.

## Open Questions

- Should tool execution be synchronous only, with tmux recommended for long-running
  processes?
  - **Recommendation**: Keep process execution simple, bounded, and transcripted first, and add supervised long-running processes only when cancellation and status requirements justify it.
- How should abort/stop propagate through async model streams and tools?
  - **Recommendation**: Propagate cancellation through one shared turn-control path so model streams, tools, and UI state settle consistently.
- What session format will be stable enough to inspect and replay?
  - **Recommendation**: Use append-only typed records with stable identifiers, final replayable content, and capped tool settlements as the inspectable baseline.

[^pi-site]: [Pi](https://pi.dev)

[^pi-docs]: [Pi documentation](https://pi.dev/docs)

[^pi-article]: ["Pi: A Coding Agent That Stays Small," Mario Zechner](https://mariozechner.at/posts/2025-11-30-pi-coding-agent/)

[^pi-github]: [earendil-works/pi](https://github.com/earendil-works/pi)
